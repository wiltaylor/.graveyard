import {Component, OnInit} from '@angular/core';
import * as signalR from '@microsoft/signalr';
import {environment} from '../../environments/environment';
import {NumberManagerService} from "../services/number-manager.service";

@Component({
  selector: 'app-home',
  templateUrl: './home.component.html',
})
export class HomeComponent implements OnInit {

  constructor(private service: NumberManagerService) {
  }

  async ngOnInit() {

  }
}
