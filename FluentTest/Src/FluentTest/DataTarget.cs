using System;
using System.Collections.Generic;

namespace FluentTest
{
    public class DataTarget<TContainer, TSut, TMock, TData>
    {
        private readonly TestConfigBase<TContainer> _config;

        public DataTarget(TestConfigBase<TContainer> config)
        {
            _config = config;
        }

        public void Data(Func<IEnumerable<TData>> dataPredicate, Action<ArrangeTarget<TContainer, TSut, TMock,TData>> testPredicate)
        {

            foreach (var d in dataPredicate())
            {
                var info = new TestInfo<TSut, TMock,TData>();
                info.Data = d;
                
                testPredicate(new ArrangeTarget<TContainer, TSut, TMock, TData>(_config, info));
            }           
        }
    }
}
