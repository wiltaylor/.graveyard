namespace FlexibleConfigEngine.Core.Script.Fluent
{
    public class GatherFluent
    {
        private readonly Graph.Gather _gather;

        public GatherFluent(Graph.Gather gather)
        {
            _gather = gather;
        }

        public GatherFluent Property(string name, string value)
        {
            if (_gather.Properties.ContainsKey(name))
                _gather.Properties[name] = value;
            else
                _gather.Properties.Add(name, value);
            return this;
        }
    }
}
