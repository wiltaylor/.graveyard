/*****************************************************************************************************
Fluent Test Build Script.
Author: Wil Taylor
*****************************************************************************************************/
#tool "nuget:?package=GitVersion.CommandLine"
#tool "nuget:?package=xunit.runner.console"
#tool "nuget:?package=ILRepack"
#tool "nuget:?package=JetBrains.dotCover.CommandLineTools"
#addin "Cake.Json"
#tool "nuget:?package=gitlink"
#tool "nuget:?package=gitreleasemanager"

var gituser = EnvironmentVariable("GITHUBUSER");
var gitpassword = EnvironmentVariable("GITHUBPASSWORD");
var gitrepoowner = "wiltaylor";
var gitreponame = "FluentTest";
var version = GitVersion(new GitVersionSettings{UpdateAssemblyInfo = true});
var target = Argument("target", "Default");
var RootDir = MakeAbsolute(Directory(".")); 
var ReleaseFolder = RootDir + "/Release";
var BuildFolder = RootDir + "/Build";
var SourceFiles = RootDir +"/Src";
var SolutionFile = SourceFiles + "/FluentTest.sln";
var ReportFolder = RootDir + "/Reports";
var NugetPackages = SourceFiles + "/packages";
var ToolsFolder = RootDir + "/tools";

//Check folder structure.
CreateDirectory(ReportFolder);
CreateDirectory(ReleaseFolder);
CreateDirectory(BuildFolder);

Task("Default")
    .IsDependentOn("Restore")
    .IsDependentOn("Package");

Task("Restore")
    .IsDependentOn("AppveyorGitVersion")
    .IsDependentOn("RestoreNuget")
    .IsDependentOn("RestoreFluentTest");

Task("Clean")
    .IsDependentOn("CleanFluentTest");

Task("Build")
    .IsDependentOn("BuildFluentTest");

Task("Test")
    .IsDependentOn("TestFluentTest")
    .IsDependentOn("CoverFluentTest");

Task("Package")
    .IsDependentOn("PackageFluentTest");

Task("Publish")
    .IsDependentOn("Package")
    .IsDependentOn("GitHubPublish")
    .IsDependentOn("PublishFluentTest");

Task("Update")
    .IsDependentOn("UpdateNuget");

/*****************************************************************************************************
Git and GitHub
*****************************************************************************************************/
Task("GitHubPublish")
    .WithCriteria(version.BranchName == "master")
    .Does(() => {
        GitReleaseManagerCreate(gituser, gitpassword, gitrepoowner, gitreponame, new GitReleaseManagerCreateSettings {
            Milestone = version.SemVer,
            Prerelease = false,
            Assets = string.Format("{0}/FluentTest.{1}.nupkg,{0}/FluentTest-{2}.zip", ReleaseFolder, version.NuGetVersionV2, version.SemVer),
            TargetCommitish = version.BranchName,
            TargetDirectory = RootDir,
            LogFilePath = ReportFolder + "/grm.log"
        });

        GitReleaseManagerPublish(gituser, gitpassword, gitrepoowner, gitreponame, version.SemVer);
    });
/*****************************************************************************************************
Appveyor Tasks
*****************************************************************************************************/
Task("AppveyorGitVersion")
    .WithCriteria(EnvironmentVariable("CI") == "True")
    .Does(() => StartProcess(ToolsFolder + "/GitVersion.CommandLine/tools/GitVersion.exe", 
        "/l console /output buildserver /updateAssemblyInfo"));

/*****************************************************************************************************
Global Nuget Tasks
*****************************************************************************************************/
Task("RestoreNuget")
    .Does(() => NuGetRestore(SolutionFile));

Task("UpdateNuget")
    .Does(() => {
        foreach(var pkgfile in GetFiles(SourceFiles + "/**/packages.config"))
            NuGetUpdate(pkgfile);
    });

/*****************************************************************************************************
FluentTest
*****************************************************************************************************/
Task("CleanFluentTest")
    .Does(() => CleanDirectory(BuildFolder + "/FluentTest"));

Task("RestoreFluentTest")
    .IsDependentOn("RestoreAssemblyInfoFluentTest")
    .IsDependentOn("RestoreAssemblyInfoFluentTest.UnitTest");

Task("RestoreAssemblyInfoFluentTest")
    .Does(() => {
            CreateDirectory(SourceFiles + "/FluentTest/Properties");

            CreateAssemblyInfo(SourceFiles + "/FluentTest/Properties/AssemblyInfo.cs", 
                new AssemblyInfoSettings { Product = "FluentTest" }); // Don't bother setting versions, gitversion overwrites them.
    }); 

Task("RestoreAssemblyInfoFluentTest.UnitTest")
    .Does(() => {
            CreateDirectory(SourceFiles + "/FluentTest.UnitTest/Properties");

            CreateAssemblyInfo(SourceFiles + "/FluentTest.UnitTest/Properties/AssemblyInfo.cs", 
                new AssemblyInfoSettings { Product = "FluentTest" }); // Don't bother setting versions, gitversion overwrites them.
        });     

Task("BuildFluentTest")
    .IsDependentOn("BuildFluentTest.GitLink");

Task("BuildFluentTest.Compile")
    .IsDependentOn("CleanFluentTest")
    .Does(() => {
        MSBuild(SolutionFile, config =>
            config.SetVerbosity(Verbosity.Minimal)
            .UseToolVersion(MSBuildToolVersion.VS2015)
            .WithTarget("FluentTest")
            .WithProperty("OutDir", BuildFolder + "/FluentTest")
            .SetMSBuildPlatform(MSBuildPlatform.x64)
            .SetPlatformTarget(PlatformTarget.MSIL));
        });

Task("BuildFluentTest.StageGitLinkFiles")
    .IsDependentOn("BuildFluentTest.Compile")
    .IsDependentOn("BuildFluentTest.UnitTest")
    .Does(() => {
        CleanDirectory(BuildFolder + "/FluentTest.GitLink");
        CopyFiles(BuildFolder + "/FluentTest/*.pdb", BuildFolder + "/FluentTest.GitLink");
        CopyFiles(BuildFolder + "/FluentTest.UnitTest/*.pdb", BuildFolder + "/FluentTest.GitLink");
    });
    

Task("BuildFluentTest.GitLink")
    .IsDependentOn("BuildFluentTest.StageGitLinkFiles")
    .Does(() => {
        GitLink(RootDir, new GitLinkSettings{
            Branch = version.BranchName,
            RepositoryUrl = "https://github.com/wiltaylor/FluentTest",
            PdbDirectoryPath = BuildFolder + "/FluentTest.GitLink"
        });

        CopyFile(BuildFolder + "/FluentTest.GitLink/FluentTest.pdb", BuildFolder + "/FluentTest/FluentTest.pdb");
    });

Task("CleanFluentTest.UnitTest")
    .Does(() => {
        CleanDirectory(BuildFolder + "/FluentTest.UnitTest");
        CleanDirectory(ReportFolder + "/FluentTest");
    });

Task("BuildFluentTest.UnitTest")
    .IsDependentOn("CleanFluentTest.UnitTest")
    .Does(() => MSBuild(SolutionFile, config =>
            config.SetVerbosity(Verbosity.Minimal)
            .UseToolVersion(MSBuildToolVersion.VS2015)
            .WithTarget("FluentTest_UnitTest")
            .WithProperty("OutDir", BuildFolder + "/FluentTest.UnitTest")
            .SetMSBuildPlatform(MSBuildPlatform.x64)
            .SetPlatformTarget(PlatformTarget.MSIL)));

Task("TestFluentTest")
    .IsDependentOn("BuildFluentTest.UnitTest")
    .Does(() => XUnit2(BuildFolder + "/FluentTest.UnitTest/FluentTest.UnitTest.dll",
        new XUnit2Settings {
        Parallelism = ParallelismOption.All,
        HtmlReport = true,
        XmlReport = true,
        NoAppDomain = false,
        OutputDirectory = ReportFolder + "/FluentTest",
        WorkingDirectory = BuildFolder + "/FluentTest.UnitTest",
        ShadowCopy = true

    }));

Task("CoverFluentTest")
    .IsDependentOn("CoverFluentTest.JSONReport")
    .IsDependentOn("CoverFluentTest.HTMLReport")
    .IsDependentOn("CoverFluentTest.Check");

Task("CleanFluentTest.Snapshot")
    .Does(() => CleanDirectory(BuildFolder + "/FluentTest.Snapshot"));

Task("CoverFluentTest.Snapshot")
    .IsDependentOn("CleanFluentTest.Snapshot")
    .IsDependentOn("BuildFluentTest.UnitTest")
    .Does(() => DotCoverCover( tool =>
        tool.XUnit2(BuildFolder + "/FluentTest.UnitTest/FluentTest.UnitTest.dll",
            new XUnit2Settings {
            Parallelism = ParallelismOption.All,
            HtmlReport = true,
            XmlReport = true,
            NoAppDomain = false,
            OutputDirectory = ReportFolder + "/FluentTest",
            WorkingDirectory = BuildFolder + "/FluentTest.UnitTest",
            ShadowCopy = true
        }),
        new FilePath(BuildFolder + "/FluentTest.Snapshot/CoverSnapshot.dcvr"),
        new DotCoverCoverSettings {
            ArgumentCustomization = args => args.Append("/ReturnTargetExitCode") 
        }));

Task("CoverFluentTest.JSONReport")
    .IsDependentOn("CoverFluentTest.Snapshot")
    .Does(() => StartProcess(ToolsFolder + "/JetBrains.dotCover.CommandLineTools/tools/dotCover.exe",
        string.Format("r /source=\"{0}\" /output=\"{1}\" /ReportType=Json", BuildFolder + "/FluentTest.Snapshot/CoverSnapshot.dcvr", ReportFolder + "/Cover.json")));

Task("CoverFluentTest.HTMLReport")
    .IsDependentOn("CoverFluentTest.Snapshot")
    .Does(() => StartProcess(ToolsFolder + "/JetBrains.dotCover.CommandLineTools/tools/dotCover.exe",
        string.Format("r /source=\"{0}\" /output=\"{1}\" /ReportType=HTML", BuildFolder + "/FluentTest.Snapshot/CoverSnapshot.dcvr", ReportFolder + "/Cover.Html")));

Task("CoverFluentTest.Check")
    .IsDependentOn("CoverFluentTest.JSONReport")
    .Does(() => {
        var coverage = (int)(ParseJsonFromFile(ReportFolder + "/Cover.json")["CoveragePercent"]);
        Information(string.Format("Code Coverage: {0}%", coverage));
        if(coverage < 80)
            throw new Exception("Cover coverage to low!");
    });

Task("PackageFluentTest")
    .IsDependentOn("PackageFluentTest.Nuget")
    .IsDependentOn("PackageFluentTest.Zip");

Task("PackageFluentTest.Nuget")
    .IsDependentOn("BuildFluentTest")
    .Does(() => NuGetPack(new NuGetPackSettings {
            Id                      = "FluentTest",
            Version                 = version.NuGetVersionV2,
            Title                   = "FluentTest",
            Authors                 = new[] {"Wil Taylor"},
            Owners                  = new[] {"Wil Taylor"},
            Description             = "Fluent Unit Test framework that allows you to define test methods as fluent lamda expressions instead of plain methods.",
            Summary                 = "Fluent Unit Test framework that allows you to define test methods as fluent lamda expressions instead of plain methods.",
            ProjectUrl              = new Uri("https://github.com/wiltaylor/FluentTest"),
            LicenseUrl              = new Uri("https://github.com/wiltaylor/FluentTest/blob/master/LICENSE.md"),
            Copyright               = "Wil Taylor 2016",
            //ReleaseNotes            = new [] {"Bug fixes", "Issue fixes", "Typos"},
            Tags                    = new [] {"UnitTest", "Xunit", "FluentTest"},
            RequireLicenseAcceptance= false,
            Symbols                 = false,
            NoPackageAnalysis       = true,
            Files                   = new [] {
                                                new NuSpecContent {Source = BuildFolder + "/FluentTest/FluentTest.dll", Target = "net45"},
                                                new NuSpecContent {Source = BuildFolder + "/FluentTest/FluentTest.pdb", Target = "net45"}
                                             },
            OutputDirectory         = ReleaseFolder
        })); 

Task("PackageFluentTest.Zip")
    .Does(() => {
        CleanDirectory(BuildFolder + "/FluentTest.Zip");
        CopyFiles(BuildFolder + "/FluentTest/*.*", BuildFolder + "/FluentTest.Zip");
        Zip(BuildFolder + "/FluentTest.Zip", ReleaseFolder + String.Format("/FluentTest-{0}.zip", version.SemVer));
    });

Task("PublishFluentTest")
    .IsDependentOn("PublishFluentTestToNuget");

Task("PublishFluentTestToNuget")
    .Does(() => NuGetPush(ReleaseFolder + string.Format("/FluentTest.{0}.nupkg", version.NuGetVersionV2),
        new NuGetPushSettings {
            Source = "https://www.nuget.org/api/v2/package",
            ApiKey = EnvironmentVariable("NUGETAPIKey")
        }));
/*****************************************************************************************************
End of script
*****************************************************************************************************/

Task("version")
    .Does(() => {
        Information("Assembly Version: " + version.AssemblySemVer);
        Information("SemVer: " + version.SemVer);
        Information("Branch: " + version.BranchName);
        Information("Commit Date: " + version.CommitDate);
        Information("Build Metadata: " + version.BuildMetaData);
        Information("PreReleaseLabel: " + version.PreReleaseLabel);
    });
RunTarget(target);


