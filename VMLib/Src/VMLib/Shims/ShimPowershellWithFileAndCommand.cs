using System;
using System.Collections;
using System.Management.Automation;
using SystemWrapper.IO;
using VMLib.IOC;
using VMLib.Utility;
using VMLib.VM;

namespace VMLib.Shims
{
    public class ShimPowershellWithFileAndCommand : ShimVirtualMachineBase
    {
        public ShimPowershellWithFileAndCommand(IVirtualMachine basevm) : base(basevm){}

        public override PowershellResults ExecutePowershell(string script, Hashtable arguments)
        {
            
            var env = ServiceDiscovery.Instance.Resolve<IEnvironmentHelper>();
            var file = ServiceDiscovery.Instance.Resolve<IFileWrap>();
            var guid = env.GetGUID();

            var hostscriptpath = $"{env.TempDir}\\{guid}.ps1";
            var guestscriptpath = $"c:\\windows\\temp\\{guid}.ps1";
            var hostdatapath = $"{env.TempDir}\\{guid}.data.xml";
            var guestdatapath = $"c:\\windows\\temp\\{guid}.data.xml";
            var hostlauncherpath = $"{env.TempDir}\\{guid}.cmd";
            var guestlauncherpath = $"c:\\windows\\temp\\{guid}.cmd";
            var hoststdoutpath = $"{env.TempDir}\\{guid}.stdout";
            var gueststdoutpath = $"c:\\windows\\temp\\{guid}.stdout";
            var hoststderrpath = $"{env.TempDir}\\{guid}.stderr";
            var gueststderrpath = $"c:\\windows\\temp\\{guid}.stderr";

            script = script.Replace("get-vmlibargs", $"Import-CliXml \"{guestdatapath}\"");

            if (arguments != null)
            {
                file.WriteAllText(hostdatapath, PSSerializer.Serialize(arguments));
                CopyToVM(hostdatapath, guestdatapath);
            }

            var lscript =
                "@\"C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" " +
                "-executionpolicy bypass -outputformat xml -noprofile -noninteractive " +
                $"-file \"c:\\windows\\temp\\{guid}.ps1\" > " +
                $"\"c:\\windows\\temp\\{guid}.stdout\" 2> " +
                $"\"c:\\windows\\temp\\{guid}.stderr\"";

            file.WriteAllText(hostlauncherpath, lscript);
            CopyToVM(hostlauncherpath, guestlauncherpath);

            file.WriteAllText(hostscriptpath, script);
            CopyToVM(hostscriptpath, guestscriptpath);

            ExecuteCommand("c:\\windows\\system32\\cmd.exe",
                $"/c \"{guestlauncherpath}\"", true, false);

            if(FileExists(gueststdoutpath))
                CopyFromVM(gueststdoutpath, hoststdoutpath);
            if (FileExists(gueststderrpath))
                CopyFromVM(gueststderrpath, hoststderrpath);

            object stdout = null;
            object stderr = null;

            if (file.Exists(hoststdoutpath))
                try { stdout = PSSerializer.Deserialize(file.ReadAllText(hoststdoutpath)
                    .Replace($"#< CLIXML{Environment.NewLine}", "")); }
                catch { stdout = null; }

            if (file.Exists(hoststderrpath))
                try { stderr = PSSerializer.Deserialize(file.ReadAllText(hoststderrpath)
                    .Replace($"#< CLIXML{Environment.NewLine}", "")); }
                catch { stderr = null; }

            if(file.Exists(hostscriptpath))
                file.Delete(hostscriptpath);
            if (file.Exists(hostdatapath))
                file.Delete(hostdatapath);
            if (file.Exists(hostlauncherpath))
                file.Delete(hostlauncherpath);
            if (file.Exists(hoststdoutpath))
                file.Delete(hoststdoutpath);
            if (file.Exists(hoststderrpath))
                file.Delete(hoststderrpath);

            if(FileExists(guestscriptpath))
                DeleteFile(guestscriptpath);
            if (FileExists(guestdatapath))
                DeleteFile(guestdatapath);
            if (FileExists(guestlauncherpath))
                DeleteFile(guestlauncherpath);
            if (FileExists(gueststdoutpath))
                DeleteFile(gueststdoutpath);
            if (FileExists(gueststderrpath))
                DeleteFile(gueststderrpath);

            return new PowershellResults(stdout, stderr);
        }
    }
}
