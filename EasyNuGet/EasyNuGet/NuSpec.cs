using System.Collections.Generic;
using System.Xml;

namespace EasyNuGet
{
    public class NuSpec
    {
        public static NuSpec Parse(string xmlText)
        {
            var result = new NuSpec();
            var xmlDoc = new XmlDocument();
            xmlDoc.LoadXml(xmlText);
            var metadata = xmlDoc.DocumentElement["metadata"];

            result.Id = metadata["id"].Value;
            result.Version = metadata["version"].Value;

            var dependancyNode = metadata["dependencies"];

            if (dependancyNode != null)
            {
                foreach (XmlElement node in dependancyNode.GetElementsByTagName("dependency"))
                {
                    result.Dependancies.Add(new Dependancy
                    {
                        Id = node.GetAttribute("id"),
                        Version = node.GetAttribute("version")
                    });
                }

                foreach (XmlElement node in dependancyNode.GetElementsByTagName("group"))
                {
                    var group = new DependancyGroup();

                    group.Name = node.GetAttribute("targetFramework") ?? "";

                    foreach (XmlElement dep in node.GetElementsByTagName("dependency"))
                    {
                        group.Dependancies.Add(new Dependancy
                        {
                            Id = dep.GetAttribute("id"),
                            Version = dep.GetAttribute("version")
                        });
                    }

                    result.Groups.Add(group);
                }
            }

            return result;
        }

        public string Id { get; internal set; }
        public string Version { get; internal set; }
        public List<Dependancy> Dependancies { get; internal set; } = new List<Dependancy>();
        public List<DependancyGroup> Groups { get; internal set; } = new List<DependancyGroup>();
    }
}
