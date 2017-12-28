using System;

namespace EasyNuGet
{
    public class Package
    {
        public string Title { get; internal set; }
        public string Version { get; internal set; }
        public string[] Tags { get; internal set; }
        public string Id { get; internal set; }
    }
}
