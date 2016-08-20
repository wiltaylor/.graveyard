using System;

namespace FluentTest
{
    public class ActTarget<TContainer, TSut, TMock, TData>
    {
        private readonly TestInfo<TContainer,TSut, TMock, TData> _info;

        public ActTarget(TestInfo<TContainer,TSut, TMock, TData> info)
        {
            _info = info;
        }

        public AssertTarget<TContainer, TSut, TMock, TData> Act(Action<TestInfo<TContainer, TSut, TMock, TData>> predicate)
        {
            predicate(_info);

            return new AssertTarget<TContainer, TSut, TMock, TData>(new AssertTestInfo<TContainer, TSut, TMock, TData>(_info));
        }
    }
}
