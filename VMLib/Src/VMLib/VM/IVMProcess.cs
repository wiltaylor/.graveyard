namespace VMLib.VM
{
    public interface IVMProcess
    {
        string Name { get; }
        ulong ProcessID { get; }
    }

    public class VMProcess : IVMProcess
    {
        public VMProcess(string name, ulong processID)
        {
            Name = name;
            ProcessID = processID;
        }

        public string Name { get; }
        public ulong ProcessID { get; }
    }
}

