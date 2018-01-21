using FlexibleConfigEngine.Core.Exceptions;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;

namespace FCE.Windows.Core
{
    [FceItem(Name="Directory")]
    public class Directory : ResourceBase
    {
        public override ResourceState Test(ConfigItem data)
        {
            var ensureText = !string.IsNullOrEmpty(data.Properties.Get("ensure")) ? data.Properties.Get("ensure").ToLower() : "present";
            var path = data.Properties.Get("path");
            var ensure = false;

            switch (ensureText)
            {
                case "present":
                    ensure = true;
                    break;
                case "absent":
                    ensure = false;
                    break;
                default:
                    throw new ResourceException("ensure property has unexpected value. Should be present or absent!");
            }

            return System.IO.Directory.Exists(path) == ensure ? ResourceState.Configured : ResourceState.NotConfigured;


        }

        public override ResourceState Apply(ConfigItem data)
        {
            var ensureText = !string.IsNullOrEmpty(data.Properties.Get("ensure")) ? data.Properties.Get("ensure").ToLower() : "present";
            var path = data.Properties.Get("path");

            switch (ensureText)
            {
                case "present":
                    System.IO.Directory.CreateDirectory(path);
                    break;
                case "absent":
                    System.IO.Directory.Delete(path, true);
                    break;
                default:
                    throw new ResourceException("ensure property has unexpected value. Should be present or absent!");
            }

            return ResourceState.Configured;
        }
    }
}
