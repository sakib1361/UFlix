namespace UFlix.Engine.Models
{
    public class SubtitleFile
    {
        public string Name { get; set; }
        public string Description { get; set; }
        public string Id { get; set; }

        private readonly Func<Task<string>> _getAction;
        public SubtitleFile(Func<Task<string>> getAction)
        {
            _getAction=getAction;
        }

        public async Task<string> GetFile()
        {
            try
            {
                return await _getAction();
            }
            catch { }
            return string.Empty;
        }
    }
}
