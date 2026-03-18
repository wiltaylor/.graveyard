using System;

namespace FlexibleConfigEngine.Core.Helper
{
    public class EnvironmentHelper : IEnvironmentHelper
    {
        public void SetExitCode(ExitCodes code)
        {
            Environment.ExitCode = (int)code;
        }
    }
}
