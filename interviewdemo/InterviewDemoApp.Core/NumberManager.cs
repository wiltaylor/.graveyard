using System.Collections.Concurrent;
using System.Collections.Immutable;

namespace InterviewDemoApp.Core;

public class NumberManager
{
    public event EventHandler<ImmutableDictionary<ulong, ulong>>? OnTick;

    private readonly ConcurrentDictionary<ulong, ulong> _numberStorage = new();
    private bool _isHalted = false;
    
    public NumberManager(int interval, ITimer timer)
    {
        if (interval < 1)
        {
            throw new ArgumentOutOfRangeException(nameof(interval), 
                "Expected a time interval larger than 0 seconds");
        }

        timer.OnTick += (sender, args) =>
        {
            if (!_isHalted)
            {
                OnTick?.Invoke(this, _numberStorage.ToImmutableDictionary());
            }
        };

    }

    private static bool IsFib(ulong number)
    {
        if (number is 0 or 1)
        {
            return true;
        }
        
        ulong first = 0;
        ulong second = 1;

        for (var i = 1; i <= 1000; i++)
        {
            ulong result = first + second;

            if (result == number)
            {
                return true;
            }

            if (result > number)
            {
                return false;
            }

            first = second;
            second = result;
        }

        return false;
    }

    public bool AddNumber(ulong number)
    {
        _numberStorage.AddOrUpdate(number, 
            n => 1, 
            (k, v) => v + 1);

        return IsFib(number);
    }

    public void Halt()
    {
        _isHalted = true;
    }

    public void Resume()
    {
        _isHalted = false;
    }

    public ImmutableDictionary<ulong, ulong> GetCounts()
    {
        return _numberStorage.ToImmutableDictionary();
    }
}