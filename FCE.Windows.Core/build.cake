#tool "nuget:?package=GitVersion.CommandLine"
#tool "nuget:?package=xunit.runner.console"
/*****************************************************************************************************
Build Script for FCE.Windowsw.Core
Author: Wil Taylor (wil@win32.io)
Created: 9/01/2018
*****************************************************************************************************/

//Folder Variables
var RepoRootFolder = MakeAbsolute(Directory("."));
var ToolsFolder = RepoRootFolder + "/Tools";

var target = Argument("target", "Default");

var nugetAPIKey = EnvironmentVariable("NUGETAPIKEY");
    
GitVersion version;

try{
    version = GitVersion(new GitVersionSettings{UpdateAssemblyInfo = true}); //This updates all AssemblyInfo files automatically.
        Information("##vso[task.setvariable variable=BuildVer]" + version.SemVer);
}
catch
{
    //Unable to get version.
}

Task("Default")
    .IsDependentOn("Restore")
    .IsDependentOn("Build");

Task("Restore")
    .IsDependentOn("FCE_Windows_Core.Restore");

Task("Clean")
    .IsDependentOn("FCE_Windows_Core.Clean");

Task("Build")
    .IsDependentOn("FCE_Windows_Core.Build");


Task("Deploy")
    .IsDependentOn("FCE_Windows_Core.Deploy");

Task("Version")
    .Does(() => {
        Information("Assembly Version: " + version.AssemblySemVer);
        Information("SemVer: " + version.SemVer);
        Information("Branch: " + version.BranchName);
        Information("Commit Date: " + version.CommitDate);
        Information("Build Metadata: " + version.BuildMetaData);
        Information("PreReleaseLabel: " + version.PreReleaseLabel);
        Information("FullBuildMetaData: " + version.FullBuildMetaData);
    });
/*****************************************************************************************************
FCE_Windows_Core
*****************************************************************************************************/
Task("FCE_Windows_Core.Restore")
    .Does(() => StartProcess("dotnet", "restore -c release"));

Task("FCE_Windows_Core.Clean")
    .IsDependentOn("FCE_Windows_Core.Clean.Windows");

Task("FCE_Windows_Core.Clean.Windows")
    .Does(() => { 
        CleanDirectory(RepoRootFolder + "/FCE.Windows.Core/bin/Release");
    });
    
Task("FCE_Windows_Core.Build")
    .IsDependentOn("FCE_Windows_Core.Build.Windows");

Task("FCE_Windows_Core.Build.Windows")
    .IsDependentOn("FCE_Windows_Core.UpdateVersion")
    .IsDependentOn("FCE_Windows_Core.Clean.Windows")
    .Does(() => {
        var ret = StartProcess("dotnet", "build -c release");

        Information("Ignore warnings about powershell file. I have not found a way to supress them, this script is used by the library not the install process");

        if(ret != 0)
            throw new Exception("Build failed!");
    });

Task("FCE_Windows_Core.UpdateVersion")
    .Does(() => {
        var file = RepoRootFolder + "/FCE.Windows.Core/FCE.Windows.Core.csproj";
        XmlPoke(file, "/Project/PropertyGroup/Version", version.SemVer);
        XmlPoke(file, "/Project/PropertyGroup/AssemblyVersion", version.AssemblySemVer);
        XmlPoke(file, "/Project/PropertyGroup/FileVersion", version.AssemblySemVer);
    });

Task("FCE_Windows_Core.Deploy")
    .IsDependentOn("FCE_Windows_Core.Deploy.NuGet");

Task("FCE_Windows_Core.Deploy.NuGet")
    .IsDependentOn("FCE_Windows_Core.Build")
    .Does(() => {
        NuGetPush(RepoRootFolder + "/FCE.Windows.Core/Bin/Release/FCE.Windows.Core." + version.SemVer + ".nupkg",
        new NuGetPushSettings{
            Source = "https://api.nuget.org/v3/index.json",
            ApiKey = nugetAPIKey
        });
    });
/*****************************************************************************************************
End of script
*****************************************************************************************************/
RunTarget(target);
