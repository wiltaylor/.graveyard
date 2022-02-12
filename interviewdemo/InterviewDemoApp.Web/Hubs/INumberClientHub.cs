namespace InterviewDemoApp.Web.Hubs;

public interface INumberClientHub
{
   Task SendMessage(string message);
}