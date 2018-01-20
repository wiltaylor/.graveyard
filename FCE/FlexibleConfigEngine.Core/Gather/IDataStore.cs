using System.Collections.Generic;

namespace FlexibleConfigEngine.Core.Gather
{
    public interface IDataStore : IDataStoreReadOnly
    {
        string GetPersistString(bool all = false);
        void Write(string name, Dictionary<string, string> data, bool persist = false);
    }

    public interface IDataStoreReadOnly
    {
        Dictionary<string, string> Read(string name);
        Dictionary<string, string> this[string name] { get; }
    }
}
