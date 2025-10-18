namespace InterviewDemoApp.Core;

/// <summary>
/// Timer interface. Used to allow unit tests to insert a fake object instead of a real timer.
/// </summary>
public interface ITimer
{
   /// <summary>
   /// Set how often in seconds the timer should be fired.
   /// </summary>
   /// <param name="interval"></param>
   void SetInterval(int interval);
   
   /// <summary>
   /// Listen to this event to handle every time the interval has elsapsed.
   /// </summary>
   event EventHandler OnTick;
}