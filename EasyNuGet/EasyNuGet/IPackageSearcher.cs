using System;
using System.Collections.Generic;
using System.Text;

namespace EasyNuGet
{
    public interface IPackageSearcher
    {
        IEnumerable<Package> Search(string query, bool preRelease = false);
    }
}
