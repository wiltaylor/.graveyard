using System.Collections.Generic;

namespace EasyNuGet
{
    public class DependancyGroup
    {
        public string Name { get; internal set; }
        public List<Dependancy> Dependancies { get; internal set; } = new List<Dependancy>();
    }
}
