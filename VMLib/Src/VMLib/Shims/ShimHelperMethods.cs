namespace VMLib.Shims
{
    public static class ShimHelperMethods
    {
        public static IVirtualMachine AddShim<T>(this IVirtualMachine vm) where T : ShimVirtualMachineBase
        {
            var constructor = typeof(T).GetConstructor(new[] {typeof(IVirtualMachine)});
            var instance = constructor?.Invoke(new object[] {vm});
            return (IVirtualMachine)instance;
        }
    }
}
