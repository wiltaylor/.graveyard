namespace FlexibleConfigEngine.Core.Helper
{
    public interface IFileSystem
    {
        bool FileExist(string path);
        void WriteFile(string path, string text);
        string ReadFile(string path);
    }
}
