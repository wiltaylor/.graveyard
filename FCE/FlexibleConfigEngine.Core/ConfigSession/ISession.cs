namespace FlexibleConfigEngine.Core.ConfigSession
{
    public interface ISession
    {
        void Apply(string script, string settings, bool testOnly = false);
        void GatherOnly(string script, string output);
        bool Validate(string script);
    }
}
