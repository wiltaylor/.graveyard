using System.IO;
using SystemWrapper.IO;
using VMLib.Exceptions;

namespace VMLib.VMware
{
    public class VMwareHypervisor : HypervisorBase
    {
        private readonly IDirectoryWrap _directory;

        public VMwareHypervisor(IDirectoryWrap directory)
        {
            _directory = directory;
        }


        public override string Name => "VMwareWorkstation";
        public override string[] GetAllVMsInFolder(string path)
        {
            return _directory.GetFiles(path, "*.vmx", SearchOption.AllDirectories);
        }
    }
}
