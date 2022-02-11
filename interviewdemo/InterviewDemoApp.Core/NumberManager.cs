using System.Collections.Concurrent;
using System.Collections.Immutable;

namespace InterviewDemoApp.Core;

public class NumberManager
{
    public event EventHandler<ImmutableDictionary<int, int>>? OnTick;

    private readonly ConcurrentDictionary<int, int> _numberStorage = new();
    public NumberManager(int interval, ITimer timer)
    {
        if (interval < 1)
        {
            throw new ArgumentOutOfRangeException(nameof(interval), 
                "Expected a time interval larger than 0 seconds");
        }

        timer.OnTick += (sender, args) =>
        {
            OnTick?.Invoke(this, _numberStorage.ToImmutableDictionary());
        };

    }

    public void AddNumber(int number)
    {
        _numberStorage.AddOrUpdate(number, 
            n => 1, 
            (e, n) => e + 1);
    }
}