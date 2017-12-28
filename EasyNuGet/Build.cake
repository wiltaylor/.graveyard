#tool "nuget:?package=GitVersion.CommandLine"

//Folder Variables
var RepoRootFolder = MakeAbsolute(Directory(".")); 
var BuildFolder = RepoRootFolder + "/Build";
var ReleaseFolder = BuildFolder + "/Release";
var SolutionFile = RepoRootFolder + "/EasyNuGet.sln";
var ToolsFolder = RepoRootFolder + "/Tools";

var nugetAPIKey = EnvironmentVariable("NUGETAPIKEY");

var target = Argument("target", "Default");

GitVersion version;

try{
    version = GitVersion(new GitVersionSettings{UpdateAssemblyInfo = true}); //This updates all AssemblyInfo files automatically.
}
catch
{
    //Unable to get version.
}

Task("Default")
    .IsDependentOn("Restore")
    .IsDependentOn("Build");

Task("Restore")
    .IsDependentOn("EasyNuGet.Restore");

Task("Clean");

Task("Build")
    .IsDependentOn("EasyNuGet.Build");

Task("Test");

Task("Deploy")
    .IsDependentOn("EasyNuGet.Deploy");

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
EasyNuGet
*****************************************************************************************************/
Task("EasyNuGet.Clean")
    .IsDependentOn("EasyNuGet.Clean.Main");

Task("EasyNuGet.Restore")
    .IsDependentOn("EasyNuGet.DotNetRestore");    

Task("EasyNuGet.Build")
    .IsDependentOn("EasyNuGet.Build.Compile");

Task("EasyNuGet.Test");

Task("EasyNuGet.Deploy")
    .IsDependentOn("EasyNuGet.Deploy.NuGet");

Task("EasyNuGet.DotNetRestore")
    .Does(() => {
        var proc = StartProcess("dotnet", new ProcessSettings { Arguments = "restore", WorkingDirectory = RepoRootFolder + "/EasyNuGet"  });

        if(proc != 0)
            throw new Exception("dotnet didn't return 0 it returned " + proc);
    });

Task("EasyNuGet.UpdateVersion")
    .Does(() => {
        var file = RepoRootFolder + "/EasyNuGet/EasyNuGet.csproj";
        XmlPoke(file, "/Project/PropertyGroup/Version", version.SemVer);
        XmlPoke(file, "/Project/PropertyGroup/AssemblyVersion", version.AssemblySemVer);
        XmlPoke(file, "/Project/PropertyGroup/FileVersion", version.AssemblySemVer);
    });

Task("EasyNuGet.Clean.Main")
    .Does(() => 
    {
        CleanDirectory(RepoRootFolder + "/EasyNuGet/Bin");
    });

Task("EasyNuGet.Build.Compile")
    .IsDependentOn("EasyNuGet.UpdateVersion")
    .IsDependentOn("EasyNuGet.Clean.Main")
    .Does(() => {
        MSBuild(SolutionFile, config =>
            config.SetVerbosity(Verbosity.Minimal)
            .SetConfiguration("Release")
            .UseToolVersion(MSBuildToolVersion.VS2017)
            .SetMSBuildPlatform(MSBuildPlatform.Automatic)
            .SetPlatformTarget(PlatformTarget.MSIL));
        });

Task("EasyNuGet.Deploy.NuGet")
    .Does(() => {
        NuGetPush(RepoRootFolder + "/EasyNuGet/Bin/Release/EasyNuGet." + version.SemVer + ".nupkg",
        new NuGetPushSettings{
            Source = "https://api.nuget.org/v3/index.json",
            ApiKey = nugetAPIKey
        });
    });
/*****************************************************************************************************
End of script
*****************************************************************************************************/
RunTarget(target);