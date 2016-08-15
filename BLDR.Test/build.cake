/*****************************************************************************************************
Template build script
Author: Wil Taylor
*****************************************************************************************************/
#tool "nuget:?package=GitVersion.CommandLine"
#tool "nuget:?package=NUnit.ConsoleRunner"

var TemplateID = "Test";
var TemplateTitle = "TestTemplate";
var Description = "Test template used to test internal functionality of BLDR";
var Authors = new[] { "Wil Taylor"};
var Owners = new[] { "Wil Taylor"};
var ProjectURL = "https://github.com/SomeUser/TestNuget/";
var IconURL = "http://cdn.rawgit.com/SomeUser/TestNuget/master/icons/testnuget.png";
var LicenseURL = "https://github.com/SomeUser/TestNuget/blob/master/LICENSE.md";
var Copyright = "Wil Taylor 2016";
var ReleaseNotes = new[] {
        "some notes here",
        "here"
    }

var version = GitVersion(new GitVersionSettings{UpdateAssemblyInfo = true});
var target = Argument("target", "Default");
var RootDir = MakeAbsolute(Directory(".")); 
var ReleaseFolder = RootDir + "/Release";
var BuildFolder = RootDir + "/Build";
var SourceFiles = RootDir +"/Src";

//Check folder structure.
CreateDirectory(ReportFolder);


Task("Default")
    .IsDependentOn("Package");

Task("Clean")
    .IsDependentOn("CleanVMLab")

Task("Package")
    .Does(() => {       
        var nuGetPackSettings   = new NuGetPackSettings {
                                        Id                      = "BLDR." + TemplateName,
                                        Version                 = version.NuGetVersionV2,
                                        Title                   = TemplateTitle,
                                        Authors                 = Authors,
                                        Owners                  = Owners,
                                        Description             = Description,
                                        Summary                 = Description,
                                        ProjectUrl              = ProjectURL,
                                        IconUrl                 = IconURL,
                                        LicenseUrl              = LicenseURL),
                                        Copyright               = Copyright,
                                        ReleaseNotes            = ReleaseNotes,
                                        Tags                    = new [] {"BLDR"},
                                        RequireLicenseAcceptance= false,
                                        Symbols                 = false,
                                        NoPackageAnalysis       = true,
                                        Files                   = new [] {
                                                                            new NuSpecContent {Source = SourceFiles + "/generator.json", Target = "bldr"},
                                                                            new NuSpecContent {Source = SourceFiles + "/scriptcs_packages.config", Target = "bldr"},
                                                                            new NuSpecContent {Source = SourceFiles + "/Project.csx", Target = "bldr"},
                                                                            new NuSpecContent {Source = SourceFiles + "/Item.csx", Target = "bldr"}
                                                                        },
                                        BasePath                = SourceFiles,
                                        OutputDirectory         = ReleaseFolder
                                    };
                    
        NuGetPack("./nuspec/TestNuget.nuspec", nuGetPackSettings);
    });

Task("Publish");




/*****************************************************************************************************
VMLab
*****************************************************************************************************/
Task("CleanVMLab")
    .Does(() => {
        CleanDirectory(BuildFolder + "/VMlab");
        CleanDirectory(BuildFolder + "/VMlab.tmp");
    });

Task("BuildVMLab")
    .IsDependentOn("CleanVMLab")
    .Does(() => {
        MSBuild(SolutionFile, config =>
            config.SetVerbosity(Verbosity.Minimal)
            .UseToolVersion(MSBuildToolVersion.VS2015)
            .WithTarget("VMLab")
            .WithProperty("OutDir", BuildFolder + "/VMLab.tmp")
            .SetMSBuildPlatform(MSBuildPlatform.x86)
            .SetPlatformTarget(PlatformTarget.MSIL));

            CopyFile(BuildFolder + "/VMLab.tmp/VMlab.dll", BuildFolder + "/VMLab/VMlab.dll");
            CopyFile(BuildFolder + "/VMLab.tmp/VMlab.pdb", BuildFolder + "/VMLab/VMlab.pdb");
            DeleteDirectory(BuildFolder + "/VMLab.tmp", true);
        });

Task("CleanVMLab.UnitTest")
    .Does(() => CleanDirectory(BuildFolder + "/VMLab.UnitTest"));

Task("BuildVMLab.UnitTest")
    .IsDependentOn("CleanVMLab.UnitTest")
    .Does(() => MSBuild(SolutionFile, config =>
            config.SetVerbosity(Verbosity.Minimal)
            .UseToolVersion(MSBuildToolVersion.VS2015)
            .WithTarget("VMLab_UnitTest")
            .WithProperty("OutDir", BuildFolder + "/VMLab.UnitTest")
            .SetMSBuildPlatform(MSBuildPlatform.x86)
            .SetPlatformTarget(PlatformTarget.MSIL)));

Task("TestVMLab")
    .IsDependentOn("BuildVMLab.UnitTest")
    .Does(() => NUnit3(BuildFolder + "/VMLab.UnitTest/VMLab.UnitTest.dll", 
        new NUnit3Settings{
            Results = ReportFolder + "/VMLab.xml"
        }));



/*****************************************************************************************************
End of script
*****************************************************************************************************/
RunTarget(target);
Information("Current Version: " + version.AssemblySemVer);