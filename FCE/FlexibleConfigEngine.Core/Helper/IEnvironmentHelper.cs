using System;
using System.Collections.Generic;
using System.Text;

namespace FlexibleConfigEngine.Core.Helper
{
    public enum ExitCodes
    {
        Ok = 0,
        Reboot = 3010,
        Error = 500
    }



    public interface IEnvironmentHelper
    {
        void SetExitCode(ExitCodes code);
    }
}
