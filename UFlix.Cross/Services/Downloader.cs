using System;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using YoutubeDLSharp;
using YoutubeDLSharp.Metadata;
using YoutubeDLSharp.Options;

namespace UFlix.Cross.Services
{
    public class Downloader
    {
        private static YoutubeDL yt;

        public static ObservableCollection<DownloadOperation> Downloads { get; } = [];

        public static async void Init()
        {

            var dir = Path.Combine(AppPaths.DataFolder, "YT");
            _=Directory.CreateDirectory(dir);
            AppConstants.YtFile = Path.Combine(dir, FFmpegUtils.YtDlpBinaryName);
            if (File.Exists(AppConstants.YtFile))
            {
                var ytdl = new YoutubeDL()
                {
                    YoutubeDLPath = AppConstants.YtFile,
                };
                var data = await ytdl.RunUpdate();
                Debug.WriteLine(data);
            }
            else
            {
                await FFmpegUtils.DownloadYtDlp(dir);
                await FFmpegUtils.DownloadFFmpeg(dir);
            }
        }
        public async Task<VideoData> GetFormats(string url)
        {
            yt ??= new YoutubeDL()
            {
                YoutubeDLPath = AppConstants.YtFile,
                FFmpegPath = Path.Combine(Path.GetDirectoryName(AppConstants.YtFile), FFmpegUtils.FfmpegBinaryName),
            };
            var res = await yt.RunVideoDataFetch(url);
            return res.Data;
        }

        internal async void AddDownload(string text, string title, string format, FormatData currectFormat)
        {
            var folder = DBUtilities.Instance.YoutubeDownloadPath;
            if (string.IsNullOrWhiteSpace(folder)) folder = Path.Combine(AppPaths.DownloadFolder, "Youtube");
            _ = Directory.CreateDirectory(folder);
            yt.OutputFolder = folder;
            var optionSet = new OptionSet
            {
                FfmpegLocation = yt.FFmpegPath
            };
            var op = new DownloadOperation()
            {
                Title = title.Trim(),
            };
            Downloads.Add(op);
            format ??= "bestvideo+bestaudio/best";
            try
            {
                var state = await yt.RunVideoDownload(text, format, output: op.OutputReport, ct: op.CancellationToken, progress: op.ProgressReport, overrideOptions: optionSet);
                var all = $"Success: {state.Success}. Format: {format}\r\n";
                if (state.ErrorOutput != null)
                    all = all+ string.Join(',', state.ErrorOutput) + Environment.NewLine;
                all+=state.Data;
                op.OutputReport.Report(all.Trim());
            }
            catch { }
        }
    }

    public class DownloadOperation : INotifyPropertyChanged
    {
        public string Title { get; set; }
        public string CurrentState { get; private set; }


        public IProgress<DownloadProgress> ProgressReport { get; }
        public IProgress<string> OutputReport { get; }
        public CancellationToken CancellationToken => ct.Token;

        public string ProgressState { get; private set; }

        private readonly CancellationTokenSource ct;

        public event PropertyChangedEventHandler PropertyChanged;

        public DownloadOperation()
        {
            ProgressReport = new Progress<DownloadProgress>(x => UpdateProgress(x));
            OutputReport = new Progress<string>(x => UpdateState(x));
            ct = new CancellationTokenSource();
        }

        private void UpdateProgress(DownloadProgress x)
        {
            ProgressState = $"{x.Progress*100:N2}%";
        }

        private void UpdateState(string state)
        {
            CurrentState = state;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(CurrentState)));
        }

        public void Stop()
        {
            ct.Cancel();
        }
    }
}
