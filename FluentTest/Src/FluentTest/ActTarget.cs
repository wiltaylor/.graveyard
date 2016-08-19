using System;

namespace FluentTest
{
    public class ActTarget<TContainer, TSut, TMock, TData>
    {
        private readonly TContainer _container;
        private readonly TestInfo<TSut, TMock, TData> _info;

        public ActTarget(TContainer container, TestInfo<TSut, TMock, TData> info)
        {
            _container = container;
            _info = info;
        }

        public AssertTarget<TSut, TMock, TData> Act(Action<TSut> predicate)
        {
            predicate(_info.Sut);

            return new AssertTarget<TSut, TMock, TData>(_info);
        }

        public AssertTarget<TSut, TMock, TData> Act(Action<TSut, TMock> predicate)
        {
            predicate(_info.Sut, _info.Mock);

            return new AssertTarget<TSut, TMock, TData>(_info);
        }

        public AssertTarget<TSut, TMock, TData> Act(Action<TSut, TMock, TData> predicate)
        {
            predicate(_info.Sut, _info.Mock, _info.Data);

            return new AssertTarget<TSut, TMock, TData>(_info);
        }
    }
}
