using System;
using System.IO;
using System.IO.Compression;
using System.Linq;
using EasyNuGet;

namespace TestApp
{
    static class Program
    {
    
        static void Main()
        {
            
            var loc = new ServiceLocator();
            var download = new PackageDownloader(loc);
            var search = new PackageSearcher(loc, download);

            //download.DownloadFromFile("D:\\Project\\Tools\\EasyNuGet\\EasyNuGet\\bin\\Release\\EasyNuGet.0.1.2.nupkg", "D:\\Sandpit\\x");

            foreach(var itm  in search.GetInstalledPackages("D:\\Sandpit\\x"))
                Console.WriteLine($"{itm.Id} - {itm.Version}");
      
      


            Console.WriteLine("done");

            Console.Read();
        }
    }
}
