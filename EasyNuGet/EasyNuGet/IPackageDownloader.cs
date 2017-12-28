using Semver;

namespace EasyNuGet
{
    public interface IPackageDownloader
    {
        void Download(string name, string version, string path, bool force = false);
        void Download(string name, SemVersion version, string path, bool force = false);
        void Download(Package package, string path, bool force = false);
        void DownloadArchive(string name, string version, string path);
        void DownloadArchive(string name, SemVersion version, string path);
        void DownloadArchive(Package package, string path);
        bool IsInstalled(string name, string version, string path);
        bool IsInstalled(string name, SemVersion version, string path);
        bool IsInstalled(Package package, string path);
    }
}
