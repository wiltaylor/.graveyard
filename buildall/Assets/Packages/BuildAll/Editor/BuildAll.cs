using UnityEditor;
using UnityEngine;
using System.Diagnostics;
using System.IO;
using System.Linq;
using Debug = UnityEngine.Debug;

public class BuildAll : EditorWindow
{
    private static readonly string ReleaseFolder = Application.dataPath + "/../Release/";

    private BuildAllSettings _settings;

    [MenuItem("File/Build All Settings", false, 3)]
    public static void OpenSettings()
    {
        GetWindow<BuildAll>();
    }

    public void OnEnable()
    {
        title = "Build All";

        _settings = (BuildAllSettings)AssetDatabase.LoadMainAssetAtPath("Assets/Packages/BuildAll/Editor/Settings.asset");
    }

    public void OnGUI()
    {         

        GUILayout.Label("Binary Name: ");
        _settings.BinaryName = GUILayout.TextField(_settings.BinaryName);

        GUILayout.Label("Release Version: ");
        _settings.Release = GUILayout.TextField(_settings.Release);

        GUILayout.Label("Publish Root Folder: ");
        _settings.PublishRoot = GUILayout.TextField(_settings.PublishRoot);

        GUILayout.Label("Package Root Folder: ");
        _settings.PackageFolder = GUILayout.TextField(_settings.PackageFolder);

        GUILayout.Label("Build Targets: ");
        _settings.BuildWindows = GUILayout.Toggle(_settings.BuildWindows, "Windows");
        _settings.BuildWindows64 = GUILayout.Toggle(_settings.BuildWindows64, "Windows 64");
        _settings.BuildLinux = GUILayout.Toggle(_settings.BuildLinux, "Linux");
        _settings.BuildWeb = GUILayout.Toggle(_settings.BuildWeb, "Web");
        _settings.BuildWebGL = GUILayout.Toggle(_settings.BuildWebGL, "WebGL");
        _settings.BuildAndroid = GUILayout.Toggle(_settings.BuildAndroid, "Android");
        _settings.BuildPackage = GUILayout.Toggle(_settings.BuildPackage, "Package");

        GUILayout.BeginHorizontal();

        if (GUILayout.Button("Build"))
        {
            SaveSettings();
            Build();
        }

        if (GUILayout.Button("Apply"))
        {
            SaveSettings();
        }

        if (GUILayout.Button("Create Folders"))
        {
            CreateFolders();
        }

        if (GUILayout.Button("Clean Up"))
        {
            if(Directory.Exists(ReleaseFolder))
                Directory.Delete(ReleaseFolder, true);

            Directory.CreateDirectory(ReleaseFolder);
        }

        GUILayout.EndHorizontal();
    }

    public void SaveSettings()
    {
        EditorUtility.SetDirty(_settings);
    }

    public static void CreateFolders()
    {
        var projectFolder = Application.dataPath + "/../";
        var folders = new[]
        {
            projectFolder + "NonUnityAssets", projectFolder + "Release", projectFolder + "Documents",
            Application.dataPath + "/Scripts", Application.dataPath + "/Scenes", Application.dataPath + "/Prefabs", 
            Application.dataPath + "/Animation", Application.dataPath + "/Sounds", Application.dataPath + "/Sprites",
            Application.dataPath + "/Models", Application.dataPath + "/Materials", Application.dataPath + "/Data"
        };

        foreach (var f in folders.Where(f => !Directory.Exists(f)))
            Directory.CreateDirectory(f);

        if (File.Exists(projectFolder + ".gitignore")) return;

        var gitfile = (from a in AssetDatabase.GetAllAssetPaths()
            where a.EndsWith(".gitignore")
            select a).First();

        File.Copy(gitfile, projectFolder + ".gitignore");

        AssetDatabase.Refresh();
    }

    [MenuItem("File/Build All", priority = 1)]
    public static void Build()
    {
        var scenes = (from s in EditorBuildSettings.scenes where s.enabled select s.path).ToArray();
        var settings = (BuildAllSettings)AssetDatabase.LoadMainAssetAtPath("Assets/Packages/BuildAll/Editor/Settings.asset");


        //Build Players - Add more or comment out as needed
        if (settings.BuildWindows)
            BuildPlayer(scenes, ReleaseFolder + "Windows", settings.BinaryName + ".exe", BuildTarget.StandaloneWindows, BuildOptions.None);
        if (settings.BuildWindows64)
            BuildPlayer(scenes, ReleaseFolder + "Windows 64", settings.BinaryName + "64.exe", BuildTarget.StandaloneWindows64, BuildOptions.None);
        if (settings.BuildWeb)
            BuildPlayer(scenes, ReleaseFolder + "Web", settings.BinaryName, BuildTarget.WebPlayer, BuildOptions.None);
        if (settings.BuildWebGL)
            BuildPlayer(scenes, ReleaseFolder + "WebGL", settings.BinaryName, BuildTarget.WebGL, BuildOptions.None);
        if (settings.BuildLinux)
            BuildPlayer(scenes, ReleaseFolder + "Linux", settings.BinaryName, BuildTarget.StandaloneLinuxUniversal, BuildOptions.None);
        if (settings.BuildAndroid)
            BuildPlayer(scenes, ReleaseFolder + "Android", settings.BinaryName + ".apk", BuildTarget.Android, BuildOptions.None);

        //Export packages 
        if (settings.BuildPackage)
            BuildPackage("Assets/Packages/BuildAll", ReleaseFolder + "Package", settings.BinaryName + ".unitypackage");

        EditorUtility.DisplayDialog("Build Complete", "", "Ok");
    }

    [MenuItem("File/Publish", priority = 2)]
    public static void PublishAll()
    {
        var settings = (BuildAllSettings)AssetDatabase.LoadMainAssetAtPath("Assets/Packages/BuildAll/Editor/Settings.asset");

        if (Directory.Exists(settings.PublishRoot))
        {
            Directory.Delete(settings.PublishRoot, true);
        }

        Directory.CreateDirectory(settings.PublishRoot);

        if (settings.BuildPackage)
            File.Copy(ReleaseFolder + "Package/" + settings.BinaryName + ".unitypackage", settings.PublishRoot + "/" + settings.BinaryName + ".unitypackage");
        if (settings.BuildAndroid)
            File.Copy(ReleaseFolder + "Android/" + settings.BinaryName + ".apk", settings.PublishRoot + "/" + settings.BinaryName + ".apk");
        if (settings.BuildWindows)
            ZipFolder(ReleaseFolder + "Windows", settings.PublishRoot + "/" + settings.BinaryName + "-Windows.zip");
        if (settings.BuildWindows64)
            ZipFolder(ReleaseFolder + "Windows 64", settings.PublishRoot + "/" + settings.BinaryName + "-Windows64.zip");
        if (settings.BuildLinux)
            ZipFolder(ReleaseFolder + "Linux", settings.PublishRoot + "/" + settings.BinaryName + "-Linux-Universal.zip");
        if (settings.BuildWebGL)
            CopyFolder(ReleaseFolder + "Web", settings.PublishRoot);
        if (settings.BuildWebGL)
            CopyFolder(ReleaseFolder + "WebGL", settings.PublishRoot + "/WebGL");

        EditorUtility.DisplayDialog("Publish Complete", "", "Ok");
    }

    static void BuildPackage(string assetfolder, string folder, string filename)
    {
        if (Directory.Exists(folder))
        {
            Directory.Delete(folder, true);
        }

        Directory.CreateDirectory(folder);

        AssetDatabase.ExportPackage(assetfolder, folder + "/" + filename, ExportPackageOptions.Recurse);
    }

    static void BuildPlayer(string[] scenes, string folder, string binary, BuildTarget target, BuildOptions options)
    {
        if (Directory.Exists(folder))
        {
            Directory.Delete(folder, true);
        }

        if (binary != "")
            BuildPipeline.BuildPlayer(scenes, folder + "/" + binary, target, options);
        else
            BuildPipeline.BuildPlayer(scenes, folder, target, options);
    }

    static void CopyFolder(string source, string target)
    {
        var sourceInfo = new DirectoryInfo(source);

        if (!Directory.Exists(target))
            Directory.CreateDirectory(target);

        foreach (var f in sourceInfo.GetFiles())
            f.CopyTo(target + "\\" + f.Name);

        foreach (var d in sourceInfo.GetDirectories())
        {
            CopyFolder(d.FullName, target + "/" + d.Name);
        }
    }

    static void ZipFolder(string source, string filename)
    {
        var zipPath = (from a in AssetDatabase.GetAllAssetPaths()
                       where a.ToLower().EndsWith("7za.exe")
                       select a).First();

        var procinfo = new ProcessStartInfo
        {
            FileName = Application.dataPath + "/../" + zipPath,
            Arguments = "a -tzip \"" + filename + "\" \"" + source + "\"",
            WindowStyle = ProcessWindowStyle.Hidden
        };

        var process = Process.Start(procinfo);
        if (process != null) process.WaitForExit();
    }

}
