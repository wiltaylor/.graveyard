using System;
using System.Collections.Generic;
using System.Text;

namespace FlexibleConfigEngine.Core.Exceptions
{
    public class MissingDependancyException :Exception
    {
        public MissingDependancyException() : base("There is an invalid dependancy in the script.")
        {
            
        }

        public MissingDependancyException(string dependancy, string itemName) : base($"{itemName} is referencing dependancy {dependancy} which doesn't exist!")
        {
            
        }
    }
}
