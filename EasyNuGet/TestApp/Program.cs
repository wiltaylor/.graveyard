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
            var search = new PackageSearcher(loc);
            var download = new PackageDownloader(loc);

            download.DownloadFromFile("D:\\Project\\Tools\\EasyNuGet\\EasyNuGet\\bin\\Release\\EasyNuGet.0.1.2.nupkg", "D:\\Sandpit\\x");
      
      


            Console.WriteLine("done");

            Console.Read();
        }
    }
}
