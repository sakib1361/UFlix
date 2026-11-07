using Microsoft.Win32.SafeHandles;

namespace NativeRunner.TorrentHandler
{
    /// <summary>
    /// Read-only view of one file of a downloading torrent. A read blocks until the piece at the
    /// current position is on disk and never returns bytes past the contiguous downloaded run
    /// (short reads). <see cref="Cancel"/> wakes every blocked read; later reads return 0.
    /// </summary>
    public class LibStream(LibTorrentHandler handler, TorrentFileInfo info, string torrentFile, long start, int pieceSize) : Stream
    {
        private const long FocusAhead = 10L * 1024 * 1024;

        private readonly LibTorrentHandler _handler = handler;
        private readonly int _piece = pieceSize;
        private readonly long _start = start;
        private readonly long _length = info.FileSize;
        private readonly int _lastPiece = (int)((start + Math.Max(info.FileSize, 1) - 1) / pieceSize);
        private readonly CancellationTokenSource _cancel = new();
        private readonly Lock _eventLock = new();
        private SafeFileHandle _file;
        private long _position;
        private bool _disposed;
        private Task _eventChain = Task.CompletedTask;

        /// <summary>Raised on a thread-pool thread (never the reader thread), in order.</summary>
        public event EventHandler BufferingStarted;
        public event EventHandler BufferingEnded;

        public override bool CanRead => true;

        public override bool CanSeek => true;

        public override bool CanWrite => false;

        public override long Length => _length;

        public override long Position
        {
            get => Interlocked.Read(ref _position);
            set => Interlocked.Exchange(ref _position, Math.Clamp(value, 0, _length));
        }

        public bool IsCancelled => _cancel.IsCancellationRequested;

        /// <summary>Wakes every blocked read and makes later reads return 0. Safe to call from any thread (mpv's cancel_fn).</summary>
        public void Cancel()
        {
            try { _cancel.Cancel(); } catch (ObjectDisposedException) { }
        }

        /// <summary>
        /// Bytes downloaded contiguously from <paramref name="position"/> (file offset). A player can pause
        /// below a low mark and resume above a high mark of runway past its playhead.
        /// </summary>
        public long ContiguousBytesFrom(long position)
        {
            position = Math.Clamp(position, 0, _length);
            return _handler.ContiguousBytes(_start + position, _start + _length, _piece);
        }

        /// <summary>Waits until the start of the file is buffered. False when cancelled or the torrent was disposed.</summary>
        public async Task<bool> InitStream()
        {
            int pieceStart = (int)(_start / _piece);
            var streamPieces = Enumerable.Range(pieceStart, 3).Where(x => x <= _lastPiece).ToArray();
            var hasStreamPieces = false;

            await _handler.SetPriority(info.Index, FileDownloadPriority.High);
            await _handler.FocusPieces(pieceStart, pieceStart + 5, _lastPiece);
            while (!IsCancelled && !_handler.IsDisposed)
            {
                var progressState = await _handler.GetFileProgress(info.Index);
                var percent = progressState * 100.0 / Math.Max(1, info.FileSize);
                var downloaded = progressState / (1024 * 1024);

                hasStreamPieces = hasStreamPieces || streamPieces.All(_handler.HasPieceNow);
                if ((percent >= 5 || downloaded > 30) && hasStreamPieces)
                {
                    break;
                }
                await Task.WhenAny(_handler.PieceArrived, Task.Delay(1000, _cancel.Token));
            }

            if (IsCancelled || _handler.IsDisposed) return false;
            _file = File.OpenHandle(torrentFile, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            return true;
        }

        private bool Unavailable => _disposed || IsCancelled || _handler.IsDisposed || _file == null;

        // how many bytes a read at the current position may return right now, 0 when the piece is missing
        private int Available(int count)
        {
            var position = Position;
            var wanted = Math.Min(count, _length - position);
            if (wanted <= 0) return 0;
            var absolute = _start + position;
            return (int)_handler.ContiguousBytes(absolute, absolute + wanted, _piece);
        }

        private void Refocus(int piece)
        {
            var pieceEnd = (int)((_start + Position + FocusAhead) / _piece);
            _handler.FocusPieces(piece, Math.Max(piece + 1, pieceEnd), _lastPiece);
        }

        private void RaiseBuffering(EventHandler handler)
        {
            if (handler == null) return;
            lock (_eventLock)
            {
                _eventChain = _eventChain.ContinueWith(_ => handler(this, EventArgs.Empty), CancellationToken.None,
                    TaskContinuationOptions.None, TaskScheduler.Default);
            }
        }

        public override int Read(byte[] buffer, int offset, int count)
        {
            ValidateBufferArguments(buffer, offset, count);
            return Read(buffer.AsSpan(offset, count));
        }

        public override int Read(Span<byte> buffer)
        {
            if (buffer.IsEmpty || Unavailable || Position >= _length) return 0;

            var available = Available(buffer.Length);
            if (available == 0)
            {
                var piece = (int)((_start + Position) / _piece);
                Refocus(piece);
                RaiseBuffering(BufferingStarted);
                try
                {
                    if (!_handler.WaitForPiece(piece, _cancel.Token)) return 0;
                }
                finally
                {
                    RaiseBuffering(BufferingEnded);
                }
                if (Unavailable) return 0;
                available = Available(buffer.Length);
                if (available == 0) return 0;
            }

            var read = RandomAccess.Read(_file, buffer[..available], Position);
            Interlocked.Add(ref _position, read);
            return read;
        }

        public override Task<int> ReadAsync(byte[] buffer, int offset, int count, CancellationToken cancellationToken)
        {
            ValidateBufferArguments(buffer, offset, count);
            return ReadAsync(buffer.AsMemory(offset, count), cancellationToken).AsTask();
        }

        public override async ValueTask<int> ReadAsync(Memory<byte> buffer, CancellationToken cancellationToken = default)
        {
            if (buffer.IsEmpty || Unavailable || Position >= _length) return 0;

            var available = Available(buffer.Length);
            if (available == 0)
            {
                var piece = (int)((_start + Position) / _piece);
                Refocus(piece);
                RaiseBuffering(BufferingStarted);
                try
                {
                    using var linked = CancellationTokenSource.CreateLinkedTokenSource(_cancel.Token, cancellationToken);
                    if (!await _handler.WaitForPieceAsync(piece, -1, linked.Token).ConfigureAwait(false))
                    {
                        cancellationToken.ThrowIfCancellationRequested();
                        return 0;
                    }
                }
                finally
                {
                    RaiseBuffering(BufferingEnded);
                }
                if (Unavailable) return 0;
                available = Available(buffer.Length);
                if (available == 0) return 0;
            }

            var read = await RandomAccess.ReadAsync(_file, buffer[..available], Position, cancellationToken).ConfigureAwait(false);
            Interlocked.Add(ref _position, read);
            return read;
        }

        public override long Seek(long offset, SeekOrigin origin)
        {
            var target = origin switch
            {
                SeekOrigin.Begin => offset,
                SeekOrigin.Current => Position + offset,
                SeekOrigin.End => _length + offset,
                _ => throw new ArgumentOutOfRangeException(nameof(origin))
            };
            if (target < 0) throw new IOException("Seek before the beginning of the stream.");
            Position = target;
            return Position;
        }

        public override void Flush()
        {
        }

        public override void SetLength(long value)
        {
            throw new NotSupportedException();
        }

        public override void Write(byte[] buffer, int offset, int count)
        {
            throw new NotSupportedException();
        }

        protected override void Dispose(bool disposing)
        {
            if (!_disposed)
            {
                _disposed = true;
                Cancel();
                _file?.Dispose();
                _file = null;
            }
            base.Dispose(disposing);
        }
    }
}
