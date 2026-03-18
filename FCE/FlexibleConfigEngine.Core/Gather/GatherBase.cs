using System;
using System.Collections.Generic;
using FlexibleConfigEngine.Core.Exceptions;

namespace FlexibleConfigEngine.Core.Gather
{
    public abstract class GatherBase : IGatherDriver
    {
        private Dictionary<string, string> _properties;

        public abstract void Run(Dictionary<string, string> properties);

        public void Run()
        {
            try
            {
                Run(_properties);
            }
            catch (GatherException)
            {
                throw;
            }
            catch (Exception e)
            {
                throw new GatherException("Exception threw an unhandled error. See inner exception for details", e);
            }
        }

        public void SetProperties(Dictionary<string, string> properties)
        {
            _properties = properties;
        }
    }
}
