using System;
using System.Collections.Generic;
using System.Collections.Specialized;
using System.Text;

namespace EasyNuGet
{
    public class PackageUploader : IPackageUploader
    {
        private readonly IServiceLocator _serviceLocator;

        public PackageUploader(IServiceLocator serviceLocator)
        {
            _serviceLocator = serviceLocator;
        }

        public void UploadPackage(string path, string key)
        {
            var url = _serviceLocator.GetService("PackagePublish/2.0.0");
            var client = new System.Net.WebClient();
            client.Headers.Add("X-NuGet-ApiKey", key);
            client.UploadFile(url, path);
            client.Dispose();
        }

        public void DeletePackage(string name, string version, string key)
        {
            var url = $"{_serviceLocator.GetService("PackagePublish/2.0.0")}/{name.ToLowerInvariant()}/{version.ToLowerInvariant()}";
            var client = new System.Net.WebClient();
            client.Headers.Add("X-NuGet-ApiKey", key);
            client.UploadValues(url, "DELETE", new NameValueCollection());
            client.Dispose();
        }
    }
}
