using System;
using System.Collections.Generic;
using System.Text;

namespace EasyNuGet
{
    public interface IPackageUploader
    {
        void UploadPackage(string path, string key);
        void DeletePackage(string name, string version, string key);
    }


}
