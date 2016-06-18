namespace VMLib.VMware
{
    public class VMwareVirtualMachine :IVirtualMachine
    {
        private readonly string _vmPath;
        private readonly IVix _vix;

        public VMwareVirtualMachine(string vmPath, IVix vix)
        {
            _vmPath = vmPath;
            _vix = vix;
        }


    }
}
