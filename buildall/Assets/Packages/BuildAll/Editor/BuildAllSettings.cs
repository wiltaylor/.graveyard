using UnityEngine;
using System.Collections;

public class BuildAllSettings : ScriptableObject
{
    public string BinaryName;
    public string Release;
    public string PublishRoot;
    public string PackageFolder;
    public bool BuildWindows;
    public bool BuildWindows64;
    public bool BuildLinux;
    public bool BuildWeb;
    public bool BuildPackage;
    public bool BuildAndroid;
    public bool BuildWebGL;
}
