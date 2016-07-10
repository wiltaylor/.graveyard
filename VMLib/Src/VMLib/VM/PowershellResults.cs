namespace VMLib.VM
{
    public class PowershellResults
    {
        public PowershellResults(object result, object error)
        {
            Result = result;
            Error = error;
        }

        public object Result { get; }
        public object Error { get; }
    }
}
