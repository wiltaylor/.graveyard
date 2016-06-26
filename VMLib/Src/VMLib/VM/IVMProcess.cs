namespace VMLib.VM
{
    public interface IVMProcess
    {
        string Name { get; }
        ulong ProcessID { get; }
    }
}

