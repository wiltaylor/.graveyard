using System;
using FluentTest.Exceptions;

namespace FluentTest
{
    public class AssertTarget<TContainer, TSut, TMock, TData>
    {
        private readonly AssertTestInfo<TContainer,TSut, TMock, TData> _info;

        public AssertTarget(AssertTestInfo<TContainer, TSut, TMock, TData> info)
        {
            _info = info;
        }

        public void Assert(Func<AssertTestInfo<TContainer, TSut, TMock, TData>, bool> predicate)
        {
            if(!predicate(_info))
                throw new FluentTestAssertException("Fluent Test assert returned false.");
        }

        public void Assert(Action<AssertTestInfo<TContainer, TSut, TMock, TData>> predicate)
        {
            predicate(_info);
        }
    }
}
