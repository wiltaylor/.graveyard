using System;
using FluentTest.Exceptions;

namespace FluentTest
{
    public class AssertTarget<TSut, TMock, TData>
    {
        private readonly TestInfo<TSut, TMock, TData> _info;

        public AssertTarget(TestInfo<TSut, TMock, TData> info)
        {
            _info = info;
        }

        public void Assert(Func<TSut, bool> predicate)
        {
            if(!predicate(_info.Sut))
                throw new FluentTestAssertException("Fluent Test assert returned false.");
        }

        public void Assert(Func<TSut, TMock, bool> predicate)
        {
            if (!predicate(_info.Sut, _info.Mock))
                throw new FluentTestAssertException("Fluent Test assert returned false.");
        }

        public void Assert(Func<TSut, TMock, TData, bool> predicate)
        {
            if (!predicate(_info.Sut, _info.Mock, _info.Data))
                throw new FluentTestAssertException("Fluent Test assert returned false.");
        }

        public void Assert(Action<TSut> predicate)
        {
            predicate(_info.Sut);
        }

        public void Assert(Action<TSut, TMock> predicate)
        {
            predicate(_info.Sut, _info.Mock);
        }

        public void Assert(Action<TSut, TMock, TData> predicate)
        {
            predicate(_info.Sut, _info.Mock, _info.Data);
        }
    }
}
