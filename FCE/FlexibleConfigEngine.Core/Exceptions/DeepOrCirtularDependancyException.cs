using System;
using System.Collections.Generic;
using System.Text;

namespace FlexibleConfigEngine.Core.Exceptions
{
    public class DeepOrCirtularDependancyException : Exception
    {
        public override string Message => "There is either a circular dependancy or extreamly deep nested depeandancy.";
    }
}
