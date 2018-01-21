using System.Net;
using FlexibleConfigEngine.Core.Exceptions;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;

namespace FCE.Windows.Core
{
    [FceItem(Name="File")]
    public class File : ResourceBase
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

            return System.IO.File.Exists(path) == ensure ? ResourceState.Configured : ResourceState.NotConfigured;
        }

        public override ResourceState Apply(ConfigItem data)
        {
            var ensureText = !string.IsNullOrEmpty(data.Properties.Get("ensure")) ? data.Properties.Get("ensure").ToLower() : "present";
            var path = data.Properties.Get("path");
            var source = data.Properties.Get("source");

            switch (ensureText)
            {
                case "present":
                    if (source.StartsWith("http://") || source.StartsWith("https://"))
                    {
                        var client = new WebClient();
                        client.DownloadFile(source, path);
                    }
                    else
                    {
                        System.IO.File.Copy(source, path);
                    }
                    break;
                case "absent":
                    System.IO.File.Delete(path);
                    break;
                default:
                    throw new ResourceException("ensure property has unexpected value. Should be present or absent!");
            }

            return ResourceState.Configured;
        }

    }
}
