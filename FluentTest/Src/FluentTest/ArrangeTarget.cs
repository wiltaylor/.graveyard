using System;
using System.Collections.Generic;

namespace FluentTest
{
    public class ArrangeTarget<TContainer, TSut, TMock, TData>
    {
        private readonly TestInfo<TContainer, TSut, TMock, TData> _info;

        public ArrangeTarget(TestInfo<TContainer, TSut, TMock, TData> info)
        {
            _info = info;
        }

        public ActTarget<TContainer, TSut, TMock, TData> Arrange(Action<TestInfo<TContainer, TSut, TMock, TData>> predicate)
        {
            predicate(_info);

            return new ActTarget<TContainer, TSut, TMock, TData>(_info);
        }

    }
}
