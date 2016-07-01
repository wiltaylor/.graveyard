using System.IO;
using DiscUtils;
using DiscUtils.Fat;
namespace VMLib.Disk
{
    public class FloppyBuilder :IFloppyBuilder
    {
        public void CreateDisk(string path, string folder)
        {
            using (var fs = File.Create(path))
            {
                using (var floppy = FatFileSystem.FormatFloppy(fs, FloppyDiskType.HighDensity, "VMLibFloppy"))
                {
                    foreach (var file in Directory.GetFiles(folder))
                    {
                        var filedata = File.ReadAllBytes(file);
                        var floppyfile = floppy.OpenFile(Path.GetFileName(file), FileMode.Create);
                        floppyfile.Write(filedata, 0, filedata.Length);
                        floppyfile.Close();
                    }
                }
            }
        }
    }
}
