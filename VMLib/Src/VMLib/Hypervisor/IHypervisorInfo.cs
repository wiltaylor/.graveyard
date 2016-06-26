namespace VMLib
{
    public interface IHypervisorInfo
    {
       string Name { get; }
        IHypervisorConnectionInfo CreateConnectionInfo();
        IHypervisor CreateHypervisor(IHypervisorConnectionInfo info);
    }
}
