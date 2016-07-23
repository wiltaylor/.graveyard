namespace VMLib.VirtualBox
{
    public class VirtualBoxHypervisor : HypervisorBase
    {
        public override string Name => "VirtualBox";
        public override string[] GetAllVMsInFolder(string path)
        {
            throw new System.NotImplementedException();
        }
    }
}
