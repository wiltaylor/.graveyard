using System.Collections.Generic;
using System.Linq;
using System.Net;
using Newtonsoft.Json.Linq;

namespace EasyNuGet
{
    public class ServiceLocator : IServiceLocator
    {
        private string _nugetFeedUrl = "https://api.nuget.org/v3/index.json";
        private readonly List<Service> _services = new List<Service>();
        private bool _notRunYet = true;

        public string NuGetFeedUri
        {
            get => _nugetFeedUrl;

            set
            {
                _notRunYet = false;
                _services.Clear();

                var client = new System.Net.WebClient();

                if(Credentials != null)
                    client.Credentials = Credentials;

                var json = JObject.Parse(client.DownloadString(value));

                foreach (var resource in (JArray) json["resources"])
                {
                    _services.Add(new Service
                    {
                        Name = resource["@type"].Value<string>(),
                        Url = resource["@id"].Value<string>()
                    });
                }

                _nugetFeedUrl = value;
            }
        }

        public IEnumerable<string> AvailableServices => _services.Select(s => s.Name);
        public string GetService(string name)
        {
            //Force nuget services to download.
            if (_notRunYet)
                NuGetFeedUri = NuGetFeedUri;

            return _services.FirstOrDefault(s => s.Name == name).Url;
        }

        public ICredentials Credentials { get; set; } = null;
    }
}
