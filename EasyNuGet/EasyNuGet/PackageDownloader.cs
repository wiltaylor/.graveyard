using System;
using System.IO;
using System.IO.Compression;
using System.Linq;

namespace EasyNuGet
{
    public class PackageDownloader : IPackageDownloader
    {
        private readonly IServiceLocator _serviceLocator;

        public PackageDownloader(IServiceLocator serviceLocator)
        {
            _serviceLocator = serviceLocator;
        }

        public void DownloadFromFile(string nugetFilePath, string path)
        {
            var spec = DownloadNuSpecFromFile(nugetFilePath);
            var targetFolder = $"{path}\\{spec.Id}\\{spec.Version}";

            ExtractPackage(spec.Id, spec.Version, new MemoryStream(File.ReadAllBytes(nugetFilePath)), targetFolder);

        }

        public void Download(string name, string version, string path, bool force = false)
        {
            var targetFolder = $"{path}\\{name}\\{version}";
            var client = new System.Net.WebClient();


            if (Directory.Exists(targetFolder))
                if(!force)
                    throw new EasyNuGetException("Package is already installed locally");
                else
                    Directory.Delete(targetFolder, true);

            var url = $"{_serviceLocator.GetService("PackageBaseAddress/3.0.0")}{name.ToLowerInvariant()}/{version.ToLowerInvariant()}/{name.ToLowerInvariant()}.{version.ToLowerInvariant()}.nupkg";
            var buffer = new MemoryStream(client.DownloadData(url));

            ExtractPackage(name, version, buffer, targetFolder);
        }

        private static void ExtractPackage(string name, string version, MemoryStream buffer, string targetFolder)
        {
            var zip = new ZipArchive(buffer);

            zip.ExtractToDirectory(targetFolder);

            if (Directory.Exists($"{targetFolder}\\package"))
                Directory.Delete($"{targetFolder}\\package", true);

            if (Directory.Exists($"{targetFolder}\\_rels"))
                Directory.Delete($"{targetFolder}\\_rels", true);

            if (File.Exists($"{targetFolder}\\[Content_Types].xml"))
                File.Delete($"{targetFolder}\\[Content_Types].xml");

            if (File.Exists($"{targetFolder}\\{name}.nuspec"))
                File.Delete($"{targetFolder}\\{name}.nuspec");

            var file = File.Create($"{targetFolder}\\{name}.{version}.nupkg");

            buffer.WriteTo(file);
            file.Close();
            buffer.Dispose();
            zip.Dispose();
        }

        public void Download(Package package, string path, bool force = false) => Download(package.Id, package.Version.ToString(), path);
        public void DownloadArchive(string name, string version, string path)
        {
            var client = new System.Net.WebClient();
            var url = $"{_serviceLocator.GetService("PackageBaseAddress/3.0.0")}{name.ToLowerInvariant()}/{version.ToLowerInvariant()}/{name.ToLowerInvariant()}.{version.ToLowerInvariant()}.nupkg";
            client.DownloadFile(url, path);
            client.Dispose();
        }

        public void DownloadArchive(Package package, string path) => DownloadArchive(package.Id, package.Version.ToString(), path);
        public NuSpec DownloadNuSpec(string name, string version)
        {
            var client = new System.Net.WebClient();
            var url = $"{_serviceLocator.GetService("PackageBaseAddress/3.0.0")}{name.ToLowerInvariant()}/{version.ToLowerInvariant()}/{name.ToLowerInvariant()}.nuspec";

            return NuSpec.Parse(client.DownloadString(url));

        }

        public NuSpec DownloadNuSpec(Package package) => DownloadNuSpec(package.Id, package.Version);
        public NuSpec DownloadNuSpecFromFile(string path)
        {
            var zip = new ZipArchive(File.OpenRead(path));
            var specEntry = zip.Entries.FirstOrDefault(e => e.FullName.Contains(".nuspec"));

            if(specEntry == null)
                throw new EasyNuGetException("Can't find nuspec manifest inside target file based nuget package!");

            var specStream = new StreamReader(specEntry.Open());
            var specText = specStream.ReadToEnd();

            specStream.Dispose();
            zip.Dispose();

            return NuSpec.Parse(specText);

        }

        public bool IsInstalled(string name, string version, string path) => Directory.Exists($"{path}\\{name}\\{version}");

        public bool IsInstalled(Package package, string path) => IsInstalled(package.Id, package.Version.ToString(), path);
    }
}
