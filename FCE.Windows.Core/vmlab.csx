VM("TestVM")
    .Memory(2048)
    .CPU(2, 1)
    .Template("Windows10x64")
    .Network(NetworkType.Bridged)
    .SharedFolder("c:\\vmlab", ".")
    .CredentialSet("MyVMCreds")
    .OnCreate((vm, session) => {
        vm.Restart();
        Echo("Disabling firewall");
        vm.Execute("netsh advfirewall set domain state off");
        vm.Execute("netsh advfirewall set private state off");
        vm.Execute("netsh advfirewall set public state off");
        Echo("Downloading Debugger");;
        vm.ExecutePowerShellCommand("Invoke-WebRequest 'https://aka.ms/vs/15/release/RemoteTools.amd64ret.enu.exe' -outFile 'c:\\RemoteTools.amd64ret.enu.exe'");
        Echo("Starting back up and installing debug tools.");
        vm.ExecutePowerShellCommand("Start-Service msiserver");
        vm.Execute("start /wait c:\\RemoteTools.amd64ret.enu.exe /install /quiet /norestart");
        Sleep(30);
        vm.ExecutePowerShellCommand("Set-Service -Name msvsmon150 -StartupType Automatic");
    });



CredentialSet("MyVMCreds")
    .Credential("administrator", "P@ssw0rd01");

