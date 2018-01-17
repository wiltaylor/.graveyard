using EasyRoslynScript;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Script.Fluent;

namespace FlexibleConfigEngine.Core.Script
{
    public static class FluentScriptMethods
    {
        public static IGraphManager GraphManager { private get; set; }

        public static GatherFluent Gather(this IScriptContext context, string name)
        {
            var gather = new Graph.Gather {Name = name};

            GraphManager.AddGather(gather);

            return new GatherFluent(gather);
        }

        public static ConfigFluent Config(this IScriptContext context, string name)
        {
            var config = new ConfigItem {Name = name};


            GraphManager.AddConfig(config);

            return new ConfigFluent(config);
        }

    }
}
