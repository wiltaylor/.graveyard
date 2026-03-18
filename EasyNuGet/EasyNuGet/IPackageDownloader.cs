
namespace EasyNuGet
{
    public interface IPackageDownloader
    {
        void DownloadFromFile(string nugetFilePath, string path);
        void Download(string name, string version, string path, bool force = false);
        void Download(Package package, string path, bool force = false);
        void DownloadArchive(string name, string version, string path);
        void DownloadArchive(Package package, string path);
        NuSpec DownloadNuSpec(string name, string version);
        NuSpec DownloadNuSpec(Package package);
        NuSpec DownloadNuSpecFromFile(string path);
        bool IsInstalled(string name, string version, string path);
        bool IsInstalled(Package package, string path);
    }
}
