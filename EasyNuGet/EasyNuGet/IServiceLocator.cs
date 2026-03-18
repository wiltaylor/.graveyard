using System;
using System.Collections.Generic;
using System.Net;
using System.Text;

namespace EasyNuGet
{
    public interface IServiceLocator
    {
        
        string NuGetFeedUri { get; set; }
        IEnumerable<string> AvailableServices { get; }
        string GetService(string name);
        ICredentials Credentials { get; set; }
    }
}
