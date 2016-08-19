using System;
using System.Collections.Generic;

namespace FluentTest
{
    public class ArrangeTarget<TContainer, TSut, TMock, TData>
    {
        private readonly TestConfigBase<TContainer> _config;
        private readonly TestInfo<TSut, TMock, TData> _info;

        public ArrangeTarget(TestConfigBase<TContainer> config, TestInfo<TSut, TMock, TData> info)
        {
            _config = config;
            _info = info;
        }

        public ActTarget<TContainer, TSut, TMock, TData> Arrange(Action<TContainer,TestInfo<TSut, TMock, TData>> predicate)
        {
            predicate(_config.Container, _info);

            return new ActTarget<TContainer, TSut, TMock, TData>(_config.Container, _info);
        }

    }
}
