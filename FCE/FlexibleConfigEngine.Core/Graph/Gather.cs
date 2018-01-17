using System;
using System.Collections.Generic;
using System.Text;

namespace FlexibleConfigEngine.Core.Graph
{
    public class Gather
    {
        public string Name { get; set; }
        public Dictionary<string, string> Properties { get; set; }= new Dictionary<string, string>();
    }
}
