using System;
using FluentTest.Exceptions;

namespace FluentTest
{
    public class AssertTarget<TContainer, TSut, TMock, TData, TResult>
    {
        private readonly AssertTestInfo<TContainer,TSut, TMock, TData, TResult> _info;

        public AssertTarget(AssertTestInfo<TContainer, TSut, TMock, TData, TResult> info)
        {
            _info = info;
        }

        public void Assert(Func<AssertTestInfo<TContainer, TSut, TMock, TData, TResult>, bool> predicate)
        {
            if(!predicate(_info))
                throw new FluentTestAssertException("Fluent Test assert returned false.");
        }

        public void Assert(Action<AssertTestInfo<TContainer, TSut, TMock, TData, TResult>> predicate)
        {
            predicate(_info);
        }
    }
}
