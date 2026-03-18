namespace VMLib.VM
{
    public interface IVMProcess
    {
        string Name { get; }
        long ProcessID { get; }
        IVirtualMachine VM { get; }
    }
}

