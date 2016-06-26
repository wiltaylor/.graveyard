/**********************************************************************************************************************************
* Build script for VMLib.
* Use build.ps1 to call this.
***********************************************************************************************************************************/
#tool "nuget:?package=NUnit.ConsoleRunner"
#addin "Cake.Powershell"

var target = Argument("target", "Default");

var RootDir = ".";
var ReleaseFolder = RootDir + "/Release";
var BuildFolder = RootDir + "/Build";
var SourceFiles = RootDir +"/Src";
var SolutionFile = SourceFiles + "/VMLib.sln";
var ReportFolder = RootDir + "/Reports";
var NugetPackages = SourceFiles + "/packages";
var MajorVersion = 0;
var MinorVersion = 0;
var PatchVersion = 0;
var BuildCount = 0;

GetCurrentVersion(out MajorVersion, out MinorVersion, out PatchVersion, out BuildCount);

var Version = string.Format("{0}.{1}.{2}.{3}", MajorVersion, MinorVersion, PatchVersion, BuildCount);
var PSVersion = string.Format("{0}.{1}.{2}", MajorVersion, MinorVersion, PatchVersion);

void GetCurrentVersion(out int Major, out int Minor, out int Patch, out int Build)
{
    var splitversion = ParseAssemblyInfo(SourceFiles + "/VMLib/Properties/AssemblyInfo.cs").AssemblyFileVersion.Split('.');

    Major = int.Parse(splitversion[0]);
    Minor = int.Parse(splitversion[1]);
    Patch = int.Parse(splitversion[2]);
    Build = int.Parse(splitversion[3]);
}

Task("Default");

Task("Version")
    .Does(() => Console.WriteLine("Version: " + Version));

Task("IncrementBuildCount")
    .Does(() => {
        BuildCount++;
        Version = string.Format("{0}.{1}.{2}.{3}", MajorVersion, MinorVersion, PatchVersion, BuildCount);
        PSVersion = string.Format("{0}.{1}.{2}", MajorVersion, MinorVersion, PatchVersion);
    });

Task("IncrementPatchVersion")
    .Does(() => {
        PatchVersion++;
        Version = string.Format("{0}.{1}.{2}.{3}", MajorVersion, MinorVersion, PatchVersion, BuildCount);
        PSVersion = string.Format("{0}.{1}.{2}", MajorVersion, MinorVersion, PatchVersion);
    });

Task("IncrementMinorVersion")
    .Does(() => {
        MinorVersion++;
        Version = string.Format("{0}.{1}.{2}.{3}", MajorVersion, MinorVersion, PatchVersion, BuildCount);
        PSVersion = string.Format("{0}.{1}.{2}", MajorVersion, MinorVersion, PatchVersion);
    });

Task("IncrementMajorVersion")
    .Does(() => {
        MajorVersion++;
        Version = string.Format("{0}.{1}.{2}.{3}", MajorVersion, MinorVersion, PatchVersion, BuildCount);
        PSVersion = string.Format("{0}.{1}.{2}", MajorVersion, MinorVersion, PatchVersion);
    });

Task("RebuildAssemblyInfo");

Task("RebuildVMLibAssemblyInfo")
    .Does(() => 
        CreateAssemblyInfo(SourceFiles + "/VMLib/Properties/AssemblyInfo.cs", new AssemblyInfoSettings {
            Product = "VMLib",
            Description = "Virtual Machine Control Library.",
            Version = Version,
            FileVersion = Version,
            Company = "Wil Taylor",
            Title = "VMLib",
            Copyright = "Copyright Wil Taylor 2016",
            ComVisible = false,
            InternalsVisibleTo = new [] {"VMLib.UnitTest", "VMLib.VMware.UnitTest", "VMLib.VirtualBox.UnitTest", "VMLib.HyperV.UnitTest"}
        }));

Task("RebuildVMLib.VMWareAssemblyInfo")
    .Does(() => 
        CreateAssemblyInfo(SourceFiles + "/VMLib.VMware/Properties/AssemblyInfo.cs", new AssemblyInfoSettings {
            Product = "VMLib.VMware",
            Description = "VMLib VMware plugin.",
            Version = Version,
            FileVersion = Version,
            Company = "Wil Taylor",
            Title = "VMLib.VMware",
            Copyright = "Copyright Wil Taylor 2016",
            ComVisible = false
        }));

Task("RebuildVMLib.HyperVAssemblyInfo")
    .Does(() => 
        CreateAssemblyInfo(SourceFiles + "/VMLib.HyperV/Properties/AssemblyInfo.cs", new AssemblyInfoSettings {
            Product = "VMLib.HyperV",
            Description = "VMLib HyperV plugin.",
            Version = Version,
            FileVersion = Version,
            Company = "Wil Taylor",
            Title = "VMLib.HyperV",
            Copyright = "Copyright Wil Taylor 2016",
            ComVisible = false
        }));

Task("RebuildVMLib.VirtualBoxAssemblyInfo")
    .Does(() => 
        CreateAssemblyInfo(SourceFiles + "/VMLib.VirtualBox/Properties/AssemblyInfo.cs", new AssemblyInfoSettings {
            Product = "VMLib.VirtualBox",
            Description = "VMLib VirtualBox plugin.",
            Version = Version,
            FileVersion = Version,
            Company = "Wil Taylor",
            Title = "VMLib.VirtualBox",
            Copyright = "Copyright Wil Taylor 2016",
            ComVisible = false
        }));

Task("Clean")
    .IsDependentOn("CleanVMLib")
    .IsDependentOn("CleanVMLib.UnitTest")
    .IsDependentOn("CleanAllHypervisors")
    .IsDependentOn("CleanAllHypervisors.UnitTest")
    .IsDependentOn("CleanZip")
    .IsDependentOn("CleanNuget")
    .IsDependentOn("CleanRelease");

Task("CleanRelease")
    .Does(() => CleanDirectory(ReleaseFolder));

Task("Restore")
    .Does(() => {
        NuGetRestore(SolutionFile);
    });

Task("BuildVMLib")
    .IsDependentOn("CleanVMLib")
    .IsDependentOn("IncrementBuildCount")
    .IsDependentOn("RebuildVMLibAssemblyInfo")
    .Does(() => {
        MSBuild(SolutionFile, config =>
            config.SetVerbosity(Verbosity.Minimal)
            .UseToolVersion(MSBuildToolVersion.VS2015)
            .WithTarget("VMLib")
            .WithProperty("OutDir", "../../" + BuildFolder + "/VMLib")
            .SetMSBuildPlatform(MSBuildPlatform.x86)
            .SetPlatformTarget(PlatformTarget.MSIL));
    });

Task("CleanVMLib")
    .Does(() => CleanDirectory(BuildFolder + "/VMLib"));

Task("BuildVMLib.UnitTest")
    .IsDependentOn("CleanVMLib.UnitTest")
    .Does(() => {
        MSBuild(SolutionFile, config =>
            config.SetVerbosity(Verbosity.Minimal)
            .UseToolVersion(MSBuildToolVersion.VS2015)
            .WithTarget("VMLib_UnitTest")
            .WithProperty("OutDir", "../../" + BuildFolder + "/VMLib.UnitTest")
            .SetMSBuildPlatform(MSBuildPlatform.x86)
            .SetPlatformTarget(PlatformTarget.MSIL));
    });

Task("CleanVMLib.UnitTest")
    .Does(() => CleanDirectory(BuildFolder + "/VMLib.UnitTest"));

Task("BuildAllHypervisors")
    .IsDependentOn("BuildVMLib.VMware")
    .IsDependentOn("BuildVMLib.VirtualBox")
    .IsDependentOn("BuildVMLib.HyperV");

Task("CleanAllHypervisors")
    .IsDependentOn("CleanVMLib.VMware")
    .IsDependentOn("CleanVMLib.VirtualBox")
    .IsDependentOn("CleanVMLib.HyperV");

Task("BuildAllHypervisors.UnitTest")
    .IsDependentOn("BuildVMLib.VMware.UnitTest")
    .IsDependentOn("BuildVMLib.VirtualBox.UnitTest")
    .IsDependentOn("BuildVMLib.HyperV.UnitTest");

Task("CleanAllHypervisors.UnitTest")
    .IsDependentOn("CleanVMLib.VMware.UnitTest")
    .IsDependentOn("CleanVMLib.VirtualBox.UnitTest")
    .IsDependentOn("CleanVMLib.HyperV.UnitTest");

Task("BuildVMLib.VMware")
    .IsDependentOn("CleanVMLib.VMware")
    .IsDependentOn("RebuildVMLib.VMWareAssemblyInfo")
    .Does(() => {        
        MSBuild(SolutionFile, config =>
            config.SetVerbosity(Verbosity.Minimal)
            .UseToolVersion(MSBuildToolVersion.VS2015)
            .WithTarget("VMLib_VMware")
            .WithProperty("OutDir", "../../" + BuildFolder + "/VMLib.VMware")
            .SetMSBuildPlatform(MSBuildPlatform.x86)
            .SetPlatformTarget(PlatformTarget.MSIL));

        DeleteFiles(BuildFolder + "/VMLib.VMware/vmlib.dll");
        DeleteFiles(BuildFolder + "/VMLib.VMware/vmlib.pdb");
    });

Task("CleanVMLib.VMware")
    .Does(() => CleanDirectory(BuildFolder + "/VMLib.VMware"));

Task("BuildVMLib.VMware.UnitTest")
    .IsDependentOn("CleanVMLib.VMware.UnitTest")
    .Does(() => {
        CleanDirectory(BuildFolder + "/VMLib.VMware.UnitTest");
        MSBuild(SolutionFile, config =>
            config.SetVerbosity(Verbosity.Minimal)
            .UseToolVersion(MSBuildToolVersion.VS2015)
            .WithTarget("VMLib_VMware_UnitTest")
            .WithProperty("OutDir", "../../" + BuildFolder + "/VMLib.VMware.UnitTest")
            .SetMSBuildPlatform(MSBuildPlatform.x86)
            .SetPlatformTarget(PlatformTarget.MSIL));
    });

Task("CleanVMLib.VMware.UnitTest")
    .Does(() => CleanDirectory(BuildFolder + "/VMLib.VMware.UnitTest"));

Task("BuildVMLib.VirtualBox")
    .IsDependentOn("CleanVMLib.VirtualBox")
    .IsDependentOn("RebuildVMLib.VirtualBoxAssemblyInfo")
    .Does(() => {
        MSBuild(SolutionFile, config =>
            config.SetVerbosity(Verbosity.Minimal)
            .UseToolVersion(MSBuildToolVersion.VS2015)
            .WithTarget("VMLib_VirtualBox")
            .WithProperty("OutDir", "../../" + BuildFolder + "/VMLib.VirtualBox")
            .SetMSBuildPlatform(MSBuildPlatform.x86)
            .SetPlatformTarget(PlatformTarget.MSIL));

        DeleteFiles(BuildFolder + "/VMLib.VirtualBox/vmlib.dll");
        DeleteFiles(BuildFolder + "/VMLib.VirtualBox/vmlib.pdb");
    });

Task("CleanVMLib.VirtualBox")
    .Does(() => CleanDirectory(BuildFolder + "/VMLib.VirtualBox"));

Task("BuildVMLib.VirtualBox.UnitTest")
    .IsDependentOn("CleanVMLib.VirtualBox.UnitTest")
    .Does(() => {
        MSBuild(SolutionFile, config =>
            config.SetVerbosity(Verbosity.Minimal)
            .UseToolVersion(MSBuildToolVersion.VS2015)
            .WithTarget("VMLib_VirtualBox_UnitTest")
            .WithProperty("OutDir", "../../" + BuildFolder + "/VMLib.VirtualBox.UnitTest")
            .SetMSBuildPlatform(MSBuildPlatform.x86)
            .SetPlatformTarget(PlatformTarget.MSIL));
    });

Task("CleanVMLib.VirtualBox.UnitTest")
    .Does(() => CleanDirectory(BuildFolder + "/VMLib.VirtualBox.UnitTest"));

Task("BuildVMLib.HyperV")
    .IsDependentOn("CleanVMLib.HyperV.UnitTest")
    .IsDependentOn("RebuildVMLib.HyperVAssemblyInfo")
    .Does(() => {
        CleanDirectory(BuildFolder + "/VMLib.HyperV");
        MSBuild(SolutionFile, config =>
            config.SetVerbosity(Verbosity.Minimal)
            .UseToolVersion(MSBuildToolVersion.VS2015)
            .WithTarget("VMLib_HyperV")
            .WithProperty("OutDir", "../../" + BuildFolder + "/VMLib.HyperV")
            .SetMSBuildPlatform(MSBuildPlatform.x86)
            .SetPlatformTarget(PlatformTarget.MSIL));

        DeleteFiles(BuildFolder + "/VMLib.HyperV/vmlib.dll");
        DeleteFiles(BuildFolder + "/VMLib.HyperV/vmlib.pdb");
    });

Task("CleanVMLib.HyperV")
    .Does(() => CleanDirectory(BuildFolder + "/VMLib.HyperV"));

Task("BuildVMLib.HyperV.UnitTest")
    .Does(() => {
        CleanDirectory(BuildFolder + "/VMLib.HyperV.UnitTest");
        MSBuild(SolutionFile, config =>
            config.SetVerbosity(Verbosity.Minimal)
            .UseToolVersion(MSBuildToolVersion.VS2015)
            .WithTarget("VMLib_HyperV_UnitTest")
            .WithProperty("OutDir", "../../" + BuildFolder + "/VMLib.HyperV.UnitTest")
            .SetMSBuildPlatform(MSBuildPlatform.x86)
            .SetPlatformTarget(PlatformTarget.MSIL));
    });

Task("CleanVMLib.HyperV.UnitTest")
    .Does(() => CleanDirectory(BuildFolder + "/VMLib.HyperV.UnitTest"));

Task("BuildPowershellModule")
    .IsDependentOn("CleanPowershellModule")
    .IsDependentOn("BuildVMLib")
    .IsDependentOn("BuildAllHypervisors")
    .Does(() =>
    {
        CopyFiles(SourceFiles + "/PSVMLib/*.ps1", BuildFolder + "/PSVMLib");
        CopyFiles(SourceFiles + "/PSVMLib/*.psd1", BuildFolder + "/PSVMLib");
        CopyFiles(SourceFiles + "/PSVMLib/*.psm1", BuildFolder + "/PSVMLib");
        CopyFiles(BuildFolder + "/VMLib/*.*", BuildFolder + "/PSVMLib");
        CopyFiles(BuildFolder + "/VMLib.VMware/*.*", BuildFolder + "/PSVMLib");
        CopyFiles(BuildFolder + "/VMLib.VirtualBox/*.*", BuildFolder + "/PSVMLib");
        CopyFiles(BuildFolder + "/VMLib.HyperV/*.*", BuildFolder + "/PSVMLib");
        DeleteFiles(BuildFolder + "/PSVMLib/*.tests.ps1");

    });

Task("CleanPowershellModule")
    .Does(() => CleanDirectory(BuildFolder + "/PSVMLib"));

Task("BuildPowershellModule.UnitTest")
    .IsDependentOn("CleanPowershellModule.UnitTest")
    .Does(() =>
    {
        CopyFiles(SourceFiles + "/PSVMLib/*.ps1", BuildFolder + "/PSVMLib.UnitTest");
        CopyFiles(SourceFiles + "/PSVMLib/Fakes/*.ps1", BuildFolder + "/PSVMLib.UnitTest/Fakes");
        CopyFiles(SourceFiles + "/PSVMLib/*.psd1", BuildFolder + "/PSVMLib.UnitTest");
        CopyFiles(SourceFiles + "/PSVMLib/*.psm1", BuildFolder + "/PSVMLib.UnitTest");
    });

Task("CleanPowershellModule.UnitTest")
    .Does(() => CleanDirectory(BuildFolder + "/PSVMLib.UnitTest"))
    .Does(() => CleanDirectory(BuildFolder + "/PSVMLib.UnitTest/Fakes"));

Task("UnitTest")
    .IsDependentOn("UnitTestVMLib")
    //.IsDependentOn("UnitTestVMLib.VMWare")
    //.IsDependentOn("UnitTestVMLib.VirtualBox")
    //.IsDependentOn("UnitTestVMLib.HyperV")
    .IsDependentOn("UnitTestPowershellModule");


Task("UnitTestVMLib")
    .IsDependentOn("BuildVMLib.UnitTest")
    .Does(() => {
        NUnit3(BuildFolder + "/VMLib.UnitTest/VMLib.UnitTest.dll");
    });

Task("UnitTestVMLib.VMWare")
    .IsDependentOn("BuildVMLib.VMware.UnitTest")
    .Does(() => {
        NUnit3(BuildFolder + "/VMLib.VMware.UnitTest/VMLib.VMware.UnitTest.dll");
    });

Task("UnitTestVMLib.VirtualBox")
    .IsDependentOn("BuildVMLib.VirtualBox.UnitTest")
    .Does(() => {
        NUnit3(BuildFolder + "/VMLib.VirtualBox.UnitTest/VMLib.VirtualBox.UnitTest.dll");
    });

Task("UnitTestVMLib.HyperV")
    .IsDependentOn("BuildVMLib.HyperV.UnitTest")
    .Does(() => {
        NUnit3(BuildFolder + "/VMLib.HyperV.UnitTest/VMLib.HyperV.UnitTest.dll");
    });

Task("UnitTestPowershellModule")
    .IsDependentOn("BuildPowershellModule.UnitTest")
    .IsDependentOn("Restore")
    .Does(() => {
        StartPowershellScript("Import-Module '" + NugetPackages + "\\Pester*\\tools\\Pester.psd1'; Invoke-Pester -script '" + BuildFolder + "\\PSVMLib.UnitTest\\*.Tests.ps1' -OutputFile '" + ReportFolder + "\\PSVMlib" + Version + ".xml' -OutputFormat NUnitXml");
    });

Task("Nuget")
    .IsDependentOn("BuildVMLib")
    .IsDependentOn("BuildAllHypervisors")
    .IsDependentOn("CleanNuget")
    .Does(() => {       
        CopyFiles(BuildFolder + "/VMLib/*.*", BuildFolder + "/VMLibNuget");
        CopyFiles(BuildFolder + "/VMLib.VMware/*.*", BuildFolder + "/VMLibNuget");
        CopyFiles(BuildFolder + "/VMLib.VirtualBox/*.*", BuildFolder + "/VMLibNuget");
        CopyFiles(BuildFolder + "/VMLib.HyperV/*.*", BuildFolder + "/VMLibNuget");

        NuGetPack(new NuGetPackSettings {
            Id = "VMLib",
            Version = Version,
            Title = "VMLib Library",
            Authors = new[] {"Wil Taylor"},
            Owners = new[] {"Wil Taylor"},
            Description = "This library creates a facade to control different hypervisors with one interface.",
            Summary = "This library creates a facade to control different hypervisors with one interface.",
            ProjectUrl = new Uri("http://github.com/wiltaylor/VMLib/"),
            //IconUrl = new Uri("http://cdn.rawgit.com/wiltaylor/VMLib/master/icons/testnuget.png"),
            LicenseUrl = new Uri("https://github.com/wiltaylor/VMLib/master/LICENSE.md"),
            Copyright = "Wil Taylor 2016",
            ReleaseNotes = new [] {"Bug fixes", "Issue Fixes", "Typos"},
            Tags = new [] {"VMLib", "VMLab", "VM", "Virtual Machine", "VMware", "VirtualBox", "HyperV" },
            RequireLicenseAcceptance = false,
            Symbols = false,
            NoPackageAnalysis = false,
            Files = new [] {
                new NuSpecContent {Source = "VMLib.dll", Target = "lib/net46"},
                new NuSpecContent {Source = "VMLib.pdb", Target = "lib/net46"},
                new NuSpecContent {Source = "VMLib.VMware.dll", Target = "lib/net46"},
                new NuSpecContent {Source = "VMLib.VMware.pdb", Target = "lib/net46"},
                new NuSpecContent {Source = "VMLib.HyperV.dll", Target = "lib/net46"},
                new NuSpecContent {Source = "VMLib.HyperV.pdb", Target = "lib/net46"},
                new NuSpecContent {Source = "VMLib.VirtualBox.dll", Target = "lib/net46"},
                new NuSpecContent {Source = "VMLib.VirtualBox.pdb", Target = "lib/net46"}
            },
            BasePath = BuildFolder + "/VMLibNuget",
            OutputDirectory = ReleaseFolder

        });
    });

Task("CleanNuget")
    .Does(() => CleanDirectory(BuildFolder + "/VMLibNuget"));

Task("ZipPackage")
    .IsDependentOn("BuildVMLib")
    .IsDependentOn("BuildAllHypervisors")
    .IsDependentOn("CleanZip")
    .Does(() => {
        CopyFiles(BuildFolder + "/VMLib/*.*", BuildFolder + "/VMLibZip");
        CopyFiles(BuildFolder + "/VMLib.VMware/*.*", BuildFolder + "/VMLibZip");
        CopyFiles(BuildFolder + "/VMLib.VirtualBox/*.*", BuildFolder + "/VMLibZip");
        CopyFiles(BuildFolder + "/VMLib.HyperV/*.*", BuildFolder + "/VMLibZip");
        Zip(BuildFolder + "/VMLibZip", ReleaseFolder + "/VMLib" + Version + ".zip");
    });

Task("CleanZip")
    .Does(() => CleanDirectory(BuildFolder + "/VMLibZip"));

Task("PowershellModule")
    .IsDependentOn("BuildPowershellModule")
    .Does(() => {
        CleanDirectory(ReleaseFolder + "/PSVMLib" + PSVersion);
        CopyFiles(BuildFolder + "/PSVMLib/*.*", ReleaseFolder + "/PSVMLib" + PSVersion);
    });

Task("Interactive")
    .IsDependentOn("PowershellModule")
    .Does(() => {
        StartProcess("powershell.exe", "-noexit -command \"Import-Module \".\\Build\\PSVMLib\\psvmlib.psd1\"");
    });

RunTarget(target);