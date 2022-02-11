namespace InterviewDemoApp.Core;

public class NumberManager
{
    public NumberManager(int interval)
    {
        if (interval < 1)
        {
            throw new ArgumentOutOfRangeException(nameof(interval), 
                "Expected a time interval larger than 0 seconds");
        } 
    }
}