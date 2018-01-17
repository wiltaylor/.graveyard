using System.IO;

namespace FlexibleConfigEngine.Core.Helper
{
    public class FileSystem : IFileSystem
    {
        public bool FileExist(string path)
        {
            return File.Exists(path);
        }

        public void WriteFile(string path, string text)
        {
            File.WriteAllText(path, text);
        }

        public string ReadFile(string path)
        {
            return File.ReadAllText(path);
        }
    }
}
