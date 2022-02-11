namespace InterviewDemoApp.Core;

/// <summary>
/// Timer Class. This is a simple wrapper around timer to allow unit testing.
/// </summary>
public class Timer: ITimer
{
    private System.Timers.Timer? _timer;
    
    /// <summary>
    /// Set the time interval that you want to be called back on.
    /// </summary>
    /// <param name="interval">Interval in seconds to trigger time.</param>
    public void SetInterval(int interval)
    {
        _timer?.Dispose();

        _timer = new System.Timers.Timer(interval);
        _timer.Enabled = true;
        _timer.Elapsed += (sender, args) => OnTick?.Invoke(sender, args);
    }

    /// <summary>
    /// This event triggers every time the interval elapses.
    /// </summary>
    public event EventHandler? OnTick;
}