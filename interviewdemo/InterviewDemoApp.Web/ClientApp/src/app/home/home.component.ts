import {Component, OnInit} from '@angular/core';
import * as signalR from '@microsoft/signalr';
import {environment} from '../../environments/environment';

@Component({
  selector: 'app-home',
  templateUrl: './home.component.html',
})
export class HomeComponent implements OnInit {

  constructor() {
  }

  async ngOnInit() {
    let connection = new signalR.HubConnectionBuilder()
      .configureLogging(signalR.LogLevel.Information)
      .withUrl( environment.apiServer + "num")
      .build();

    connection.start().then(() => {
      console.log("connected");
    }).catch(err => {
      console.log(err);
    });

    connection.on("SendMessage", () => {
      console.log("Message");
    });

  }

}
