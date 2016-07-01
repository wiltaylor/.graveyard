namespace VMLib.VM
{
    public class VMProcess : IVMProcess
    {
        public VMProcess(string name, long processID, IVirtualMachine vm)
        {
            Name = name;
            ProcessID = processID;
            VM = vm;
        }

        public string Name { get; }
        public long ProcessID { get; }
        public IVirtualMachine VM { get; }
    }
}