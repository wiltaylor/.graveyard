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
    };

var version = GitVersion(new GitVersionSettings{UpdateAssemblyInfo = true});
var target = Argument("target", "Default");
var RootDir = MakeAbsolute(Directory(".")); 
var ReleaseFolder = RootDir + "/Release";
var BuildFolder = RootDir + "/Build";
var SourceFiles = RootDir +"/Src";

Task("Default")
    .IsDependentOn("Package");

Task("Clean")
    .Does(() => CleanFolder(ReleaseFolder));

Task("Package")
    .Does(() => {       
        var nuGetPackSettings   = new NuGetPackSettings {
                                        Id                      = "BLDR." + TemplateID,
                                        Version                 = version.NuGetVersionV2,
                                        Title                   = TemplateTitle,
                                        Authors                 = Authors,
                                        Owners                  = Owners,
                                        Description             = Description,
                                        Summary                 = Description,
                                        ProjectUrl              = new Uri(ProjectURL),
                                        IconUrl                 = new Uri(IconURL),
                                        LicenseUrl              = new Uri(LicenseURL),
                                        Copyright               = Copyright,
                                        ReleaseNotes            = ReleaseNotes,
                                        Tags                    = new [] {"BLDR"},
                                        RequireLicenseAcceptance= false,
                                        Symbols                 = false,
                                        NoPackageAnalysis       = true,
                                        Files                   = new [] {
                                                                            new NuSpecContent {Source = SourceFiles + "/generator.json", Target = "bldr"},
                                                                            /*new NuSpecContent {Source = SourceFiles + "/scriptcs_packages.config", Target = "bldr"},*/
                                                                            new NuSpecContent {Source = SourceFiles + "/Project.csx", Target = "bldr"},
                                                                            new NuSpecContent {Source = SourceFiles + "/Item.csx", Target = "bldr"}
                                                                        },
                                        BasePath                = SourceFiles,
                                        OutputDirectory         = ReleaseFolder
                                    };
                    
        NuGetPack(nuGetPackSettings);
    });

Task("Publish")
    .IsDependentOn()
    .Does(() => {
        NugetPush(ReleaseFolder + string.Format("/BLDR.{0}.{1}.nupkg", TemplateID, NugetVer),
            new NuGetPushSettings {
                Source = "https://www.nuget.org/api/v2/package",
                ApiKey = EnvironmentVariable("NUGETAPIKey")
        });
    });


/*****************************************************************************************************
End of script
*****************************************************************************************************/
RunTarget(target);
Information("Current Version: " + version.AssemblySemVer);