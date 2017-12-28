using System.Collections.Generic;
using System.Linq;
using Newtonsoft.Json.Linq;

namespace EasyNuGet
{
    public class PackageSearcher : IPackageSearcher
    {
        private readonly IServiceLocator _serviceLocator;

        public PackageSearcher(IServiceLocator serviceLocator)
        {
            _serviceLocator = serviceLocator;
        }

        public IEnumerable<Package> Search(string query, bool preRelease = false)
        {
            var searchUrl = $"{_serviceLocator.GetService("SearchQueryService")}?q={query}&prerelease={preRelease}&semVerLevel=2.0.0";
            var client = new System.Net.WebClient();
            var json = JObject.Parse(client.DownloadString(searchUrl));

            return (from pkg in (JArray)json["data"]
                    from ver in (JArray)pkg["versions"]
                    let tags = ((JArray)pkg["tags"]).Select(t => t.Value<string>()).ToArray()
                    let authors = ((JArray)pkg["authors"]).Select(t => t.Value<string>()).ToArray()
                    select new Package
                    {
                        Id = pkg["id"].Value<string>(),
                        Title = pkg["title"].Value<string>(),
                        Tags = tags,
                        Version = ver["version"].Value<string>()
                    }).ToList();
        }
    }
}
