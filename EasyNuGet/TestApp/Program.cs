using System;
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
            var results = search.Search("id:nuget.client").FirstOrDefault();

                Console.WriteLine($"{results.Id} - {results.Version}");

            var spec = download.DownloadNuSpec(results);

            foreach (var s in spec.Dependancies)
            {
                Console.WriteLine($"Dep: {s.Id} - {s.Version}");

                var spec2 = download.DownloadNuSpec(s.Id, s.Version);

                foreach (var x in spec2.Dependancies)
                    Console.WriteLine($"Dep Dep: {x.Id} - {x.Version}");

            }


            Console.WriteLine("done");

            Console.Read();
        }
    }
}
