namespace VMLib.VM
{
    public class VMProcess : IVMProcess
    {
        public VMProcess(string name, long processID)
        {
            Name = name;
            ProcessID = processID;
        }

        public string Name { get; }
        public long ProcessID { get; }
    }
}