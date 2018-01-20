using System;
using FlexibleConfigEngine.Core.Exceptions;
using FlexibleConfigEngine.Core.Graph;

namespace FlexibleConfigEngine.Core.Resource
{
    public abstract class ResourceBase : IResourceDriver
    {
        private ConfigItem _configItem;

        public void GetData(ConfigItem data)
        {
            _configItem = data;
        }

        public ResourceState Test()
        {
            try
            {
                return Test(_configItem);
            }
            catch (ResourceException ex)
            {
                throw ex;
            }
            catch (Exception ex)
            {
                throw new ResourceException("Test method threw an unhandled exception, see inner exception for details", ex);
            }
            
        }

        public abstract ResourceState Test(ConfigItem data);
        public abstract ResourceState Apply(ConfigItem data);


        public ResourceState Apply()
        {
            try
            {
                return Apply(_configItem);
            }
            catch (ResourceException ex)
            {
                throw ex;
            }
            catch (Exception ex)
            {
                throw new ResourceException("Apply method threw an unhandled exception, see inner exception for details", ex);
            }
        }
    }
}
