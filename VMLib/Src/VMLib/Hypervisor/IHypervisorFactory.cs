using System.Collections.Generic;

namespace VMLib
{
    public interface IHypervisorFactory
    {
        /// <summary>
        /// Returns the names of all the hypervisors supported by VMLib.
        /// </summary>
        /// <returns>List of supported hypervisors.</returns>
        string[] GetHypervisorNames();

        /// <summary>
        /// Creates a connection info object for target hypervisor. Use this object to set hypervisor specific settings
        /// for the hypervisor when creating it.
        /// </summary>
        /// <param name="hypervisorName">Name of hypervisor to return settings object for. Use GetHyperVisorNames() to retrive valid values.</param>
        /// <returns>An instance of the hypervisor connection info with default values prepopulated.</returns>
        IHypervisorConnectionInfo CreateHypervisorConnectionInfo(string hypervisorName);

        /// <summary>
        /// Creates an instance of a hypervisor object.
        /// </summary>
        /// <param name="hypervisorName">Name of hypervisor to create object for. Use GetHyperVisorNames() to retrive valid values.</param>
        /// <param name="connectioninfo">Contains all the settings used to create the hypervisor object. Use CreateHypervisorConnectionInfo object to create an instance.</param>
        /// <returns>Hypervisor object used to interact with the hypervisor.</returns>
        IHypervisor CreateHypervisor(string hypervisorName, IHypervisorConnectionInfo connectioninfo);
    }
}
