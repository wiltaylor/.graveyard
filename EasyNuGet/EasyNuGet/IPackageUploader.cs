using System;
using System.Collections.Generic;
using System.Text;
using Semver;

namespace EasyNuGet
{
    public interface IPackageUploader
    {
        void UploadPackage(string path, string key);
        void DeletePackage(string name, string version, string key);
        void DeletePackage(string name, SemVersion version, string key);
    }


}
