namespace VMLib.HyperV
{
    public class HyperVHypervisor : HypervisorBase
    {
        public override string Name => "HyperV";
        public override string[] GetAllVMsInFolder(string path)
        {
            throw new System.NotImplementedException();
        }

        public override string Extension => "xml";
    }
}
