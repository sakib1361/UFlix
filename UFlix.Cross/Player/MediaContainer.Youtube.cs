using MpvSharp;
using UFlix.Cross.Services;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using TrackType = UFlix.Engine.Models.TrackType;

namespace UFlix.Cross.Player
{
    public partial class MediaContainer
    {
        private TrackItem _youtubeAudio;

        /// <summary>YouTube serves video and audio separately: mpv plays the chosen video stream with the audio as an external track.</summary>
        private async Task OpenYoutube(MediaItem media, TimeSpan? start)
        {
            HasPlaylist = true;
            var found = DBUtilities.Instance.UseYtDLFirst
                ? await TryAndRunWithYTDL(media) || await TryAndRunWithNative(media)
                : await TryAndRunWithNative(media) || await TryAndRunWithYTDL(media);
            if (!found || MediaTracks.Count == 0) throw new InvalidOperationException("No playable YouTube stream found");

            foreach (var track in MediaTracks.Where(x => x.TrackType == TrackType.Video))
                track.SetAction(() => SetYoutubeTrack(track));

            var dbCodec = DBUtilities.Instance.YoutubeCodec ?? "avc";
            var dbWidth = DBUtilities.Instance.YoutubeWidth;
            dbWidth = dbWidth == 0 ? 1080 : dbWidth;
            var videos = MediaTracks.Where(x => x.TrackType == TrackType.Video).ToList();
            var httpTracks = videos.Where(x => x.Description.EndsWith("HLS") == false);
            var videoTrack = httpTracks.FirstOrDefault(x => x.Width == dbWidth && x.Codec != null && x.Codec.StartsWith(dbCodec))
                ?? httpTracks.FirstOrDefault(x => x.Width == dbWidth)
                ?? videos.FirstOrDefault(x => x.Width == dbWidth && x.Codec != null && x.Codec.StartsWith(dbCodec))
                ?? videos.FirstOrDefault(x => x.Width == dbWidth)
                ?? videos.FirstOrDefault(x => x.Codec != null && x.Codec.StartsWith(dbCodec))
                ?? videos.FirstOrDefault();
            _youtubeAudio = MediaTracks.FirstOrDefault(x => x.TrackType == TrackType.Audio);
            CurrentSubTrack = videoTrack;
            await LoadYoutubeTrack(media, videoTrack, start);
        }

        private Task LoadYoutubeTrack(MediaItem media, TrackItem video, TimeSpan? start) =>
            _player.LoadAsync(video.Address, new LoadOptions { Title = media.Title, Start = start, AudioUrl = _youtubeAudio?.Address });

        private async void SetYoutubeTrack(TrackItem track)
        {
            if (track == null || _player == null) return;
            var position = _player.Position;
            DBUtilities.Instance.YoutubeCodec = track.Codec;
            DBUtilities.Instance.YoutubeWidth = (int)track.Width;
            CurrentSubTrack = track;
            try
            {
                await LoadYoutubeTrack(CurrentMedia, track, position);
                _player.Volume = _volume;
                Play();
            }
            catch (Exception ex)
            {
                NavigationHelper.Notify(ex.Message);
            }
        }
    }
}
