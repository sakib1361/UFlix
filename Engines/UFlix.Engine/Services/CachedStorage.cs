namespace UFlix.Engine.Services
{
    public class CachedStorage
    {
        public string Id { get; set; }
        public string Data { get; set; }
        public DateTime ExpiresOn { get; set; }
    }

    public class PermanentStorage
    {
        public string Id { get; set; }
        public string Data { get; set; }
    }

    public class UserSetting
    {
        public string Id { get; set; }
        public string UserId { get; set; }
        public string Value { get; set; }
    }
}
