namespace VMLib.Shims
{
    public class ShimRenameFolderByShell : ShimVirtualMachineBase
    {
        public ShimRenameFolderByShell(IVirtualMachine basevm) : base(basevm) { }

        public override void RenameDirectory(string path, string newPath)
        {
            Basevm.ExecuteCommand("c:\\windows\\system32\\cmd.exe", $"/c move /Y \"{path}\" \"{newPath}\"", true, false);
        }
    }
}
