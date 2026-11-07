using System;
using System.IO;
using System.Threading.Tasks;
using UFlix.Engine.Models;

namespace UFlix.Cross.TorrentEngines
{
    public interface ITorrentEngine : IDisposable
    {
        Task<Stream> OpenTorrent(MediaItem item, PlayerInformation info);
        Task<Stream> OpenNextTorrent(MediaItem item);
        void SetMetaData(bool isEnabled);
        int TotalFiles { get; }
        string CurrentFile { get; }
    }
}
