using UnityEngine;
using System.Collections;
using UnityEditor;

public static class ScriptableObjectUtility {

    public static void CreateAsset<T>() where T : ScriptableObject
    {
        var asset = ScriptableObject.CreateInstance<T>();
        ProjectWindowUtil.CreateAsset(asset, "New " + typeof(T).Name + ".asset");
    }

    //[MenuItem("Assets/Create/BuildAllSettings")]
    public static void CreateYourScriptableObject()
    {
        CreateAsset<BuildAllSettings>();
    }
}
