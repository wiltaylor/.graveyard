namespace InterviewDemoApp.Web.Hubs;

/// <summary>
/// This interface is the SignalR RPC that can be sent to the client.
/// </summary>
public interface INumberClientHub
{
   /// <summary>
   /// Sends a simple text message to the client which will be displayed.
   /// </summary>
   /// <param name="message">Message to send to the client.</param>
   /// <returns>No return data.</returns>
   Task SendMessage(string message);
}