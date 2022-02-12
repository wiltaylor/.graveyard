namespace InterviewDemoApp.Web.Hubs;

public interface INumberClientHub
{
   void SendMessage(string message);
}