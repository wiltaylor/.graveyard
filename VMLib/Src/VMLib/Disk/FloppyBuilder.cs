using System.IO;
using DiscUtils;
using DiscUtils.Fat;
using Serilog;

namespace VMLib.Disk
{
    public class FloppyBuilder :IFloppyBuilder
    {
        private readonly ILogger _log;

        public FloppyBuilder(ILogger log)
        {
            _log = log;
        }


        public void CreateDisk(string path, string folder)
        {
            _log.Information("Creating Disk path: {path} folder {folder}", path, folder);
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
